using System.Buffers.Binary;

namespace Nara.SourceToolHost;

internal static class Program
{
    private const int MaxImageBytes = 16 * 1024 * 1024;
    private const ulong MaxPackageBytes = 64 * 1024 * 1024;

    private static int Main(string[] args)
    {
        try
        {
            if (args.Length != 0)
                throw new InvalidDataException();

            // Read only the envelope. The tool opens raw stdin itself to consume the remaining
            // unchanged wire request through EOF. Do not buffer, read ahead, or close this stream.
            Stream input = Console.OpenStandardInput();
            Span<byte> header = stackalloc byte[20];
            input.ReadExactly(header);
            uint version = BinaryPrimitives.ReadUInt32LittleEndian(header[8..12]);
            uint count = BinaryPrimitives.ReadUInt32LittleEndian(header[12..16]);
            uint entryIndex = BinaryPrimitives.ReadUInt32LittleEndian(header[16..20]);
            if (!header[..8].SequenceEqual("NARATOOL"u8)
                || version != 1 || count is < 1 or > 64 || entryIndex >= count)
                throw new InvalidDataException();

            byte[][] images = new byte[(int)count][];
            Span<byte> lengthBytes = stackalloc byte[8];
            ulong totalBytes = 0;
            for (int index = 0; index < images.Length; index++)
            {
                input.ReadExactly(lengthBytes);
                ulong length = BinaryPrimitives.ReadUInt64LittleEndian(lengthBytes);
                if (length is < 1 or > MaxImageBytes || length > MaxPackageBytes - totalBytes)
                    throw new InvalidDataException();
                totalBytes += length;
                images[index] = new byte[(int)length];
                input.ReadExactly(images[index]);
            }

            int result = ManagedTool.Run(images, (int)entryIndex);
            GC.KeepAlive(input);
            return result;
        }
        catch (Exception)
        {
            // Package data, reflection errors and exception messages are never diagnostics.
            // Bootstrap stdout belongs exclusively to the finite tool protocol.
            try
            {
                Console.OpenStandardError().Write("Nara managed tool bootstrap failed.\n"u8);
            }
            catch (Exception)
            {
                // A closed diagnostic pipe must still produce a nonzero process result.
            }
            return 1;
        }
    }
}
