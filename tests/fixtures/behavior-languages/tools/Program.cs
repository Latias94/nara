using System.Text.Json;
using System.Text.Json.Serialization;

namespace Nara.SourceTools;

internal static class Program
{
    private const int MaxRequestBytes = 64 * 1024;
    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
    };

    private static int Main(string[] args)
    {
        try
        {
            string? fault = ParseFault(args);
            ToolRequest request = JsonSerializer.Deserialize<ToolRequest>(ReadRequest(), Options)
                ?? throw new InvalidOperationException("Missing request");
            if (request.ProtocolVersion != 1 || request.Name is null
                || request.Fields is null || request.Parameters is null
                || request.Operation is not ("describe" or "attach" or "edit" or "spawn"))
                throw new InvalidOperationException("Invalid request");

            switch (fault)
            {
                case "malformed":
                    Console.OpenStandardOutput().Write("{"u8);
                    return 0;
                case "stdout_limit":
                    WriteOversized(Console.OpenStandardOutput());
                    return 0;
                case "stderr_limit":
                    WriteOversized(Console.OpenStandardError());
                    break;
                case "nonzero":
                    throw new InvalidOperationException("Requested process failure");
                case "hold":
                    Thread.Sleep(300);
                    break;
            }

            object result = request.Operation == "describe"
                ? new ToolDescription("description", EnemyStats.Describe())
                : EnemyStats.Propose(request);
            byte[] response = JsonSerializer.SerializeToUtf8Bytes(
                new ToolResponse(1, request.Request, result), Options);
            Console.OpenStandardOutput().Write(response);
            return 0;
        }
        catch (Exception)
        {
            Console.Error.WriteLine("EnemyStats tool request failed.");
            return 1;
        }
    }

    private static string? ParseFault(string[] args) => args.Length switch
    {
        0 => null,
        2 when args[0] == "--fault" && args[1] is
            ("malformed" or "stdout_limit" or "stderr_limit" or "nonzero" or "hold") => args[1],
        _ => throw new InvalidOperationException("Invalid tool arguments"),
    };

    private static ReadOnlySpan<byte> ReadRequest()
    {
        byte[] bytes = new byte[MaxRequestBytes + 1];
        using Stream input = Console.OpenStandardInput();
        int length = 0;
        while (length < bytes.Length)
        {
            int count = input.Read(bytes.AsSpan(length));
            if (count == 0)
                return bytes.AsSpan(0, length);
            length += count;
        }
        throw new InvalidOperationException("Request exceeds the byte limit");
    }

    private static void WriteOversized(Stream output)
    {
        byte[] bytes = new byte[MaxRequestBytes + 1];
        Array.Fill(bytes, (byte)'x');
        output.Write(bytes);
    }
}
