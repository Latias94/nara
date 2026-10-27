using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Runtime.Loader;
using System.Text.Json;

namespace Nara.BehaviorTrial;

public static class Bridge
{
    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        UnmappedMemberHandling = System.Text.Json.Serialization.JsonUnmappedMemberHandling.Disallow,
    };
    private static readonly Dictionary<long, LiveModule> Modules = [];
    private static readonly Dictionary<long, WeakReference> Retired = [];
    private static readonly Dictionary<long, IBehaviorModule> Leases = [];
    private static long nextId;
    private static int ownerThread;
    private static bool executing;

    // All operations are synchronous and confined to the first caller's managed thread.
    [UnmanagedCallersOnly]
    public static unsafe int Control(byte* input, int inputLength, byte* output, int capacity)
    {
        if (inputLength < 0 || capacity < 0
            || (input == null && inputLength != 0) || (output == null && capacity != 0))
            return -1;
        int currentThread = Environment.CurrentManagedThreadId;
        int previousOwner = Interlocked.CompareExchange(ref ownerThread, currentThread, 0);
        if ((previousOwner != 0 && previousOwner != currentThread) || executing)
            return -1;

        executing = true;
        try
        {
            ControlRequest request = JsonSerializer.Deserialize<ControlRequest>(
                new ReadOnlySpan<byte>(input, inputLength), Options)
                ?? throw new InvalidOperationException("Missing control request");
            byte[] response = Dispatch(request, capacity);
            response.CopyTo(new Span<byte>(output, capacity));
            return response.Length;
        }
        catch (ResponseCapacityException)
        {
            return -2;
        }
        catch (Exception)
        {
            // Never retain an authored exception, reflection value, or its collectible types.
            return -1;
        }
        finally
        {
            executing = false;
        }
    }

    private static byte[] Dispatch(ControlRequest request, int capacity)
    {
        switch (request.Operation)
        {
            case "load":
                return Load(request.Path ?? throw new InvalidOperationException("Missing path"), capacity);
            case "describe":
                return Serialize(DescribeModule(GetModule(ModuleId(request))), capacity);
            case "invoke":
                return Serialize(InvokeModule(GetModule(ModuleId(request)), Snapshot(request)), capacity);
            case "retain":
                return Retain(ModuleId(request), capacity);
            case "probe_lease":
                return Serialize(InvokeModule(GetLease(LeaseId(request)), Snapshot(request)), capacity);
            case "retire":
            {
                byte[] response = Serialize(new CompletionResponse(false), capacity);
                Retire(ModuleId(request));
                return response;
            }
            case "poll":
                return Serialize(new CompletionResponse(Poll(ModuleId(request))), capacity);
            case "release":
            {
                byte[] response = Serialize(new EmptyResponse(), capacity);
                if (!Leases.Remove(LeaseId(request)))
                    throw new InvalidOperationException("Unknown lease");
                return response;
            }
            default:
                throw new InvalidOperationException("Unknown control operation");
        }
    }

    private static long ModuleId(ControlRequest request) =>
        request.Module ?? throw new InvalidOperationException("Missing module");

    private static long LeaseId(ControlRequest request) =>
        request.Lease ?? throw new InvalidOperationException("Missing lease");

    private static SchemaSnapshot Snapshot(ControlRequest request) =>
        request.Snapshot ?? throw new InvalidOperationException("Missing snapshot");

    private static IBehaviorModule GetModule(long id) =>
        Modules.TryGetValue(id, out LiveModule? live)
            ? live.Module
            : throw new InvalidOperationException("Unknown or retired module");

    private static IBehaviorModule GetLease(long id) =>
        Leases.TryGetValue(id, out IBehaviorModule? module)
            ? module
            : throw new InvalidOperationException("Unknown lease");

    private static byte[] Serialize<T>(T value, int capacity)
    {
        byte[] response = JsonSerializer.SerializeToUtf8Bytes(value, Options);
        if (response.Length > capacity)
            throw new ResponseCapacityException();
        return response;
    }

    private static byte[] Load(string path, int capacity)
    {
        long id = checked(++nextId);
        // Check fixed response capacity before publishing an otherwise unreachable root.
        byte[] response = Serialize(new ModuleResponse(id), capacity);
        Modules.Add(id, LoadCandidate(Path.GetFullPath(path)));
        return response;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static LiveModule LoadCandidate(string path)
    {
        var resolver = new AssemblyDependencyResolver(path);
        var context = new ModuleLoadContext(resolver);
        try
        {
            Assembly assembly = context.LoadBytes(path);
            Type type = assembly.GetType("Nara.AuthoredBehaviors.Module", throwOnError: true)!;
            var module = Activator.CreateInstance(type) as IBehaviorModule
                ?? throw new InvalidOperationException("Module does not implement IBehaviorModule");
            return new LiveModule(context, module);
        }
        catch
        {
            // Constructor side effects are arbitrary; requesting unload is not rollback.
            context.Unload();
            throw;
        }
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static SchemaBinding[] DescribeModule(IBehaviorModule module) =>
        module.Describe().Select(binding => new SchemaBinding(binding.Schema, binding.Version)).ToArray();

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static NameCommand[] InvokeModule(IBehaviorModule module, SchemaSnapshot snapshot) =>
        module.Invoke(snapshot).Select(command => new NameCommand(command.Target, command.Value)).ToArray();

    private static byte[] Retain(long moduleId, int capacity)
    {
        IBehaviorModule module = GetModule(moduleId);
        long id = checked(++nextId);
        byte[] response = Serialize(new LeaseResponse(id), capacity);
        Leases.Add(id, module);
        return response;
    }

    private static void Retire(long id)
    {
        if (Retired.ContainsKey(id))
            return;
        if (!Modules.Remove(id, out LiveModule? live))
            throw new InvalidOperationException("Unknown module");
        Retired.Add(id, live.Retire());
    }

    private static bool Poll(long id)
    {
        if (!Retired.TryGetValue(id, out WeakReference? context))
            throw new InvalidOperationException("Unknown or not retired module");
        CollectOnce();
        return !IsContextAlive(context);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void CollectOnce()
    {
        // One collection attempt, not a deadline, forced unload, or an RSS claim.
        GC.Collect();
        GC.WaitForPendingFinalizers();
        GC.Collect();
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static bool IsContextAlive(WeakReference context) => context.IsAlive;

    private sealed record ControlRequest(
        string Operation,
        string? Path = null,
        long? Module = null,
        long? Lease = null,
        SchemaSnapshot? Snapshot = null);
    private sealed record ModuleResponse(long Module);
    private sealed record LeaseResponse(long Lease);
    private sealed record CompletionResponse(bool Complete);
    private sealed record EmptyResponse;
    private sealed class ResponseCapacityException : Exception { }

    private sealed class LiveModule
    {
        private ModuleLoadContext? context;
        private IBehaviorModule? module;

        public LiveModule(ModuleLoadContext context, IBehaviorModule module)
        {
            this.context = context;
            this.module = module;
        }

        public IBehaviorModule Module =>
            module ?? throw new InvalidOperationException("Module retired");

        [MethodImpl(MethodImplOptions.NoInlining)]
        public WeakReference Retire()
        {
            ModuleLoadContext previous = context
                ?? throw new InvalidOperationException("Module retired");
            var weak = new WeakReference(previous, trackResurrection: true);
            module = null;
            context = null;
            previous.Unload();
            return weak;
        }
    }

    private sealed class ModuleLoadContext(AssemblyDependencyResolver resolver)
        : AssemblyLoadContext(isCollectible: true)
    {
        protected override Assembly? Load(AssemblyName assemblyName)
        {
            Assembly contract = typeof(IBehaviorModule).Assembly;
            if (assemblyName.Name == contract.GetName().Name)
            {
                if (!AssemblyName.ReferenceMatchesDefinition(assemblyName, contract.GetName()))
                    throw new InvalidOperationException("Incompatible contract assembly identity");
                return contract;
            }

            string? dependencyPath = resolver.ResolveAssemblyToPath(assemblyName);
            return dependencyPath is null ? null : LoadBytes(dependencyPath);
        }

        public Assembly LoadBytes(string path)
        {
            // No authored assembly file handle survives a load, including same-path rebuilds.
            using FileStream stream = File.OpenRead(path);
            return LoadFromStream(stream);
        }
    }
}
