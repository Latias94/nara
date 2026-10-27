using System.Runtime.InteropServices;
using System.Text.Json;

namespace Nara.BehaviorTrial;

public sealed record NameCommand(string Target, string Value);

public static class Bridge
{
    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        UnmappedMemberHandling = System.Text.Json.Serialization.JsonUnmappedMemberHandling.Disallow,
    };

    // The authored callback sees owned DTOs, never these borrowed native buffers.
    [UnmanagedCallersOnly]
    public static unsafe int Invoke(byte* input, int inputLength, byte* output, int capacity)
    {
        try
        {
            if (inputLength < 0 || capacity < 0)
                return -1;
            SchemaSnapshot request = JsonSerializer.Deserialize<SchemaSnapshot>(
                new ReadOnlySpan<byte>(input, inputLength), Options)
                ?? throw new InvalidOperationException("Missing invocation");
            byte[] response = JsonSerializer.SerializeToUtf8Bytes(SchemaBehaviors.Invoke(request), Options);
            if (response.Length > capacity)
                return -2;
            response.CopyTo(new Span<byte>(output, capacity));
            return response.Length;
        }
        catch (Exception)
        {
            return -1;
        }
    }

    [UnmanagedCallersOnly]
    public static unsafe int Describe(byte* input, int inputLength, byte* output, int capacity)
    {
        try
        {
            if (inputLength < 0 || capacity < 0)
                return -1;
            byte[] response = JsonSerializer.SerializeToUtf8Bytes(SchemaBehaviors.Describe(), Options);
            if (response.Length > capacity)
                return -2;
            response.CopyTo(new Span<byte>(output, capacity));
            return response.Length;
        }
        catch (Exception)
        {
            return -1;
        }
    }
}
