using System.Text.Json;
using System.Text.Json.Nodes;

namespace Nara.SourceTools;

internal static class EnemyStats
{
    internal static JsonNode Describe()
    {
        using Stream resource = typeof(EnemyStats).Assembly.GetManifestResourceStream(
            "enemy-stats.catalog.json")
            ?? throw new InvalidOperationException("Missing component catalog");
        JsonNode catalog = JsonNode.Parse(resource)
            ?? throw new InvalidOperationException("Invalid component catalog");
#if TOOL_REVISION_INVALID
        catalog["payload"]!["components"]![0]!["version"] = 2;
#endif
        return catalog;
    }

    internal static ToolProposal Propose(ToolRequest request)
    {
        string title = request.Operation switch
        {
            "attach" => $"Attach EnemyStats to {request.Name}",
            "edit" => $"Edit EnemyStats for {request.Name}",
            "spawn" => $"Spawn {request.Name} with EnemyStats",
            _ => throw new InvalidOperationException("Unknown EnemyStats operation"),
        };
        string? fault = request.Parameters.TryGetValue("fault", out JsonElement faultValue)
            ? Text(faultValue)
            : null;
        if (fault == "throw")
            throw new InvalidOperationException("Requested proposal failure");
        long health = Integer(request.Fields["health"]);
        long armor = Integer(request.Fields["armor"]);
        long bonus = request.Parameters.TryGetValue("bonus", out JsonElement bonusValue)
            ? Integer(bonusValue)
            : 0;
#if TOOL_REVISION_TWO
        bonus = checked(bonus * 2);
#endif
        health = checked(health + bonus);
        List<ToolWrite> writes =
        [
            new(new("health", []), new("i64", health)),
            new(new("armor", []), new("i64", armor)),
        ];
        if (fault == "bad_last_value")
            writes[1] = new(new("armor", []), new("string", "invalid armor"));
        if (fault == "too_many_writes")
        {
            while (writes.Count < 65)
                writes.Add(writes[0]);
        }
        return new ToolProposal("proposal", title, writes.ToArray());
    }

    private static long Integer(JsonElement value)
    {
        if (value.GetProperty("type").GetString() != "i64"
            || !value.GetProperty("value").TryGetInt64(out long integer))
            throw new InvalidOperationException("Expected an i64 component value");
        return integer;
    }

    private static string Text(JsonElement value)
    {
        if (value.GetProperty("type").GetString() != "string"
            || value.GetProperty("value").ValueKind != JsonValueKind.String)
            throw new InvalidOperationException("Expected a string component value");
        return value.GetProperty("value").GetString()!;
    }
}

internal sealed record ToolRequest
{
    public required uint ProtocolVersion { get; init; }
    public required ulong Request { get; init; }
    public required string Operation { get; init; }
    public required string Name { get; init; }
    public required Dictionary<string, JsonElement> Fields { get; init; }
    public required Dictionary<string, JsonElement> Parameters { get; init; }
}

internal sealed record ToolResponse(uint ProtocolVersion, ulong Request, object Result);
internal sealed record ToolDescription(string Kind, JsonNode Catalog);
internal sealed record ToolProposal(string Kind, string Title, ToolWrite[] Writes);
internal sealed record ToolWrite(ToolProperty Property, ToolValue Value);
internal sealed record ToolProperty(string Root, string[] Steps);
internal sealed record ToolValue(string Type, object Value);
