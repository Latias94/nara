using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.Metadata.Ecma335;
using System.Reflection.PortableExecutable;
using System.Runtime.Loader;

namespace Nara.SourceToolHost;

// This is a captured-image adapter for a fully trusted finite tool, not a sandbox.
// The bootstrap, runtime and framework paths belong to the Host's trusted platform baseline.
internal sealed class ManagedTool : AssemblyLoadContext
{
    private readonly Dictionary<string, PackageImage> package;
    private readonly Dictionary<string, FrameworkImage> framework;

    private ManagedTool(
        Dictionary<string, PackageImage> package,
        Dictionary<string, FrameworkImage> framework)
        : base("Nara finite source tool", isCollectible: false)
    {
        this.package = package;
        this.framework = framework;
    }

    internal static int Run(byte[][] bytes, int entryIndex)
    {
        Dictionary<string, FrameworkImage> framework = ReadFramework();
        Dictionary<string, PackageImage> package = new(StringComparer.OrdinalIgnoreCase);
        string bootstrapName = typeof(ManagedTool).Assembly.GetName().Name
            ?? throw new FileLoadException();
        PackageImage[] images = new PackageImage[bytes.Length];
        for (int index = 0; index < bytes.Length; index++)
        {
            PackageImage image = Inspect(bytes[index], index == entryIndex);
            string name = image.Identity.Name!;
            if (StringComparer.OrdinalIgnoreCase.Equals(name, bootstrapName)
                || framework.ContainsKey(name) || !package.TryAdd(name, image))
                throw new FileLoadException();
            images[index] = image;
        }

        // Validate every declared edge before any package LoadFromStream. ALC caches by simple
        // name and can satisfy a lower-version request without calling Load again.
        foreach (PackageImage image in images)
        {
            foreach (AssemblyName reference in image.References)
            {
                if (package.TryGetValue(reference.Name!, out PackageImage? dependency))
                {
                    if (!Matches(dependency.Identity, reference, allowFrameworkUnification: false))
                        throw new FileLoadException();
                }
                else if (!framework.TryGetValue(reference.Name!, out FrameworkImage? baseline)
                    || !Matches(baseline.Identity, reference, allowFrameworkUnification: true))
                {
                    throw new FileLoadException();
                }
            }
        }

        ManagedTool context = new(package, framework);
        Assembly assembly = context.LoadFromAssemblyName(images[entryIndex].Identity);
        MethodInfo entry = assembly.EntryPoint ?? throw new FileLoadException();
        // Invoke the PE entry point, including private Main. No original argv, Assembly.Location,
        // GetEntryAssembly identity, apartment setup or async entry semantics are promised.
        return entry.Invoke(null, new object[] { Array.Empty<string>() }) is int result
            ? result : throw new FileLoadException();
    }

    protected override Assembly Load(AssemblyName assemblyName)
    {
        string name = assemblyName.Name ?? throw new FileLoadException();
        if (package.TryGetValue(name, out PackageImage? image))
        {
            if (!Matches(image.Identity, assemblyName, allowFrameworkUnification: false))
                throw new FileLoadException();
            using MemoryStream stream = new(image.Bytes, writable: false);
            return LoadFromStream(stream);
        }

        if (framework.TryGetValue(name, out FrameworkImage? baseline)
            && Matches(baseline.Identity, assemblyName, allowFrameworkUnification: true))
        {
            // Reuse CoreLib and other already loaded baseline assemblies. Otherwise select only
            // the explicit TPA path; never ask the default context to probe an arbitrary name.
            foreach (Assembly loaded in Default.Assemblies)
            {
                if (Matches(loaded.GetName(), baseline.Identity, allowFrameworkUnification: false))
                    return loaded;
            }
            Assembly loadedBaseline = Default.LoadFromAssemblyPath(baseline.Path);
            if (!Matches(loadedBaseline.GetName(), baseline.Identity, allowFrameworkUnification: false))
                throw new FileLoadException();
            return loadedBaseline;
        }

        // Null and FileNotFoundException allow fallback to other resolvers/default probing.
        throw new FileLoadException();
    }

    protected override nint LoadUnmanagedDll(string unmanagedDllName) => throw new FileLoadException();

    private static Dictionary<string, FrameworkImage> ReadFramework()
    {
        string paths = AppContext.GetData("TRUSTED_PLATFORM_ASSEMBLIES") as string
            ?? throw new FileLoadException();
        string bootstrapPath = Path.GetFullPath(typeof(ManagedTool).Assembly.Location);
        string runtimeDirectory = Path.GetDirectoryName(typeof(object).Assembly.Location)
            ?? throw new FileLoadException();
        StringComparer pathComparer = OperatingSystem.IsWindows()
            ? StringComparer.OrdinalIgnoreCase : StringComparer.Ordinal;
        Dictionary<string, FrameworkImage> result = new(StringComparer.OrdinalIgnoreCase);
        foreach (string path in paths.Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            string fullPath = Path.GetFullPath(path);
            // The admitted baseline is Microsoft.NETCore.App. Do not promote application TPA
            // siblings or the bootstrap itself into the framework set.
            if (pathComparer.Equals(fullPath, bootstrapPath)
                || !pathComparer.Equals(Path.GetDirectoryName(fullPath), runtimeDirectory))
                continue;
            AssemblyName identity = AssemblyName.GetAssemblyName(fullPath);
            if (identity.Name is null)
                throw new FileLoadException();
            if (result.TryGetValue(identity.Name, out FrameworkImage? existing))
            {
                // The runtime may repeat the same CoreLib TPA record. That is one baseline
                // image; a second path or conflicting identity is still ambiguous and rejects.
                if (!pathComparer.Equals(existing.Path, fullPath)
                    || !Matches(existing.Identity, identity, allowFrameworkUnification: false))
                    throw new FileLoadException();
            }
            else
            {
                result.Add(identity.Name, new(identity, fullPath));
            }
        }
        AssemblyName core = typeof(object).Assembly.GetName();
        if (!result.TryGetValue(core.Name!, out FrameworkImage? coreImage)
            || !Matches(coreImage.Identity, core, allowFrameworkUnification: false))
            throw new FileLoadException();
        return result;
    }

    private static PackageImage Inspect(byte[] bytes, bool isEntry)
    {
        using MemoryStream stream = new(bytes, writable: false);
        using PEReader pe = new(stream);
        CorHeader cor = pe.PEHeaders.CorHeader ?? throw new BadImageFormatException();
        if (!pe.HasMetadata || (cor.Flags & CorFlags.ILOnly) == 0
            || (cor.Flags & CorFlags.NativeEntryPoint) != 0
            || cor.ManagedNativeHeaderDirectory.Size != 0
            || cor.ManagedNativeHeaderDirectory.RelativeVirtualAddress != 0)
            throw new BadImageFormatException();

        MetadataReader reader = pe.GetMetadataReader();
        // Multi-module assemblies and file-backed resources require a package filesystem namespace.
        if (!reader.IsAssembly || reader.AssemblyFiles.Count != 0)
            throw new BadImageFormatException();
        AssemblyDefinition definition = reader.GetAssemblyDefinition();
        AssemblyName identity = ReadIdentity(reader, definition.Name, definition.Version,
            definition.Culture, definition.PublicKey, definition.Flags);
        AssemblyName[] references = new AssemblyName[reader.AssemblyReferences.Count];
        int index = 0;
        foreach (AssemblyReferenceHandle handle in reader.AssemblyReferences)
        {
            AssemblyReference reference = reader.GetAssemblyReference(handle);
            references[index++] = ReadIdentity(reader, reference.Name, reference.Version,
                reference.Culture, reference.PublicKeyOrToken, reference.Flags);
        }

        int entryToken = cor.EntryPointTokenOrRelativeVirtualAddress;
        if (entryToken != 0)
            ValidateEntry(reader, entryToken);
        else if (isEntry)
            throw new BadImageFormatException();
        return new(bytes, identity, references);
    }

    private static AssemblyName ReadIdentity(MetadataReader reader, StringHandle name,
        Version version, StringHandle culture, BlobHandle key, AssemblyFlags flags)
    {
        // Retargetable/Windows Runtime identities do not belong to this exact managed graph.
        if ((flags & (AssemblyFlags.Retargetable | AssemblyFlags.ContentTypeMask)) != 0)
            throw new BadImageFormatException();
        string simpleName = reader.GetString(name);
        if (string.IsNullOrWhiteSpace(simpleName))
            throw new BadImageFormatException();
        AssemblyName identity = new()
        {
            Name = simpleName,
            Version = version,
            CultureName = reader.GetString(culture),
        };
        byte[] keyBytes = reader.GetBlobBytes(key);
        if ((flags & AssemblyFlags.PublicKey) != 0)
        {
            if (keyBytes.Length == 0)
                throw new BadImageFormatException();
            identity.SetPublicKey(keyBytes);
        }
        else
        {
            if (keyBytes.Length is not (0 or 8))
                throw new BadImageFormatException();
            identity.SetPublicKeyToken(keyBytes);
        }
        // Validate/compute the token while still in metadata admission, before executing code.
        _ = identity.GetPublicKeyToken();
        return identity;
    }

    private static bool Matches(AssemblyName actual, AssemblyName requested, bool allowFrameworkUnification)
    {
        if ((requested.Flags & AssemblyNameFlags.Retargetable) != 0
            || requested.ContentType != AssemblyContentType.Default
            || !StringComparer.OrdinalIgnoreCase.Equals(actual.Name, requested.Name)
            || !StringComparer.OrdinalIgnoreCase.Equals(actual.CultureName ?? "", requested.CultureName ?? "")
            || !(actual.GetPublicKeyToken() ?? []).AsSpan().SequenceEqual(requested.GetPublicKeyToken() ?? [])
            || actual.Version is null || requested.Version is null)
            return false;
        return allowFrameworkUnification
            ? actual.Version >= requested.Version : actual.Version == requested.Version;
    }

    private static void ValidateEntry(MetadataReader reader, int token)
    {
        EntityHandle handle = MetadataTokens.EntityHandle(token);
        if (handle.Kind != HandleKind.MethodDefinition)
            throw new BadImageFormatException();
        MethodDefinition method = reader.GetMethodDefinition((MethodDefinitionHandle)handle);
        if (reader.GetString(method.Name) != "Main"
            || (method.Attributes & MethodAttributes.Static) == 0
            || (method.Attributes & (MethodAttributes.Abstract | MethodAttributes.PinvokeImpl)) != 0
            || (method.ImplAttributes & MethodImplAttributes.CodeTypeMask) != MethodImplAttributes.IL
            || (method.ImplAttributes & MethodImplAttributes.ManagedMask) != MethodImplAttributes.Managed
            || (method.ImplAttributes & MethodImplAttributes.InternalCall) != 0
            || method.RelativeVirtualAddress == 0 || method.GetGenericParameters().Count != 0)
            throw new BadImageFormatException();
        TypeDefinitionHandle declaringType = method.GetDeclaringType();
        int remainingTypes = reader.TypeDefinitions.Count;
        while (!declaringType.IsNil)
        {
            if (remainingTypes-- == 0)
                throw new BadImageFormatException();
            TypeDefinition type = reader.GetTypeDefinition(declaringType);
            if (type.GetGenericParameters().Count != 0)
                throw new BadImageFormatException();
            declaringType = type.GetDeclaringType();
        }

        BlobReader signature = reader.GetBlobReader(method.Signature);
        if (signature.ReadSignatureHeader().RawValue != 0
            || signature.ReadCompressedInteger() != 1
            || signature.ReadSignatureTypeCode() != SignatureTypeCode.Int32
            || signature.ReadSignatureTypeCode() != SignatureTypeCode.SZArray
            || signature.ReadSignatureTypeCode() != SignatureTypeCode.String
            || signature.RemainingBytes != 0)
            throw new BadImageFormatException();

        foreach (CustomAttributeHandle attributeHandle in method.GetCustomAttributes())
        {
            CustomAttribute attribute = reader.GetCustomAttribute(attributeHandle);
            EntityHandle owner = attribute.Constructor.Kind switch
            {
                HandleKind.MemberReference => reader.GetMemberReference(
                    (MemberReferenceHandle)attribute.Constructor).Parent,
                HandleKind.MethodDefinition => reader.GetMethodDefinition(
                    (MethodDefinitionHandle)attribute.Constructor).GetDeclaringType(),
                _ => throw new BadImageFormatException(),
            };
            StringHandle attributeName;
            StringHandle attributeNamespace;
            if (owner.Kind == HandleKind.TypeReference)
            {
                TypeReference type = reader.GetTypeReference((TypeReferenceHandle)owner);
                attributeName = type.Name;
                attributeNamespace = type.Namespace;
            }
            else if (owner.Kind == HandleKind.TypeDefinition)
            {
                TypeDefinition type = reader.GetTypeDefinition((TypeDefinitionHandle)owner);
                attributeName = type.Name;
                attributeNamespace = type.Namespace;
            }
            else
            {
                throw new BadImageFormatException();
            }
            string name = reader.GetString(attributeName);
            string ns = reader.GetString(attributeNamespace);
            if ((ns == "System" && name == "STAThreadAttribute")
                || (ns == "System.Runtime.CompilerServices" && name == "AsyncStateMachineAttribute"))
                throw new BadImageFormatException();
        }
    }

    private sealed record PackageImage(byte[] Bytes, AssemblyName Identity, AssemblyName[] References);
    private sealed record FrameworkImage(AssemblyName Identity, string Path);
}
