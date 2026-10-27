using System.Globalization;
using System.Text.Json;

namespace Nara.BehaviorTrial;

public sealed record SchemaNativeTarget(string Id, string Name);
public sealed record SchemaSnapshot(
    string Schema,
    string Actor,
    Dictionary<string, JsonElement> Fields,
    SchemaNativeTarget[] Targets,
    string Fault);
public sealed record SchemaBinding(string Schema, uint Version);

public static class SchemaBehaviors
{
    private static readonly Dictionary<string, Func<SchemaSnapshot, List<NameCommand>>> Callbacks =
        new(StringComparer.Ordinal)
        {
            ["trial.behavior.Mover"] = Move,
            ["trial.behavior.Sensor"] = Signal,
            ["trial.behavior.Label"] = Label,
        };

    public static SchemaBinding[] Describe() => Callbacks.Keys
        .OrderBy(schema => schema, StringComparer.Ordinal)
        .Select(schema => new SchemaBinding(schema, 1))
        .ToArray();

    public static List<NameCommand> Invoke(SchemaSnapshot input)
    {
        if (input.Fault == "throw")
            throw new InvalidOperationException("Trial callback failure");
        if (!Callbacks.TryGetValue(input.Schema, out var callback))
            throw new InvalidOperationException($"Unknown schema '{input.Schema}'");

        List<NameCommand> commands = callback(input);
        if (input.Fault == "bad_target")
            commands.Add(new("unavailable", "bad"));
        return commands;
    }

    private static List<NameCommand> Move(SchemaSnapshot input)
    {
        double amount = Number(input, "amount");
        double gain = input.Fields.ContainsKey("gain") ? Number(input, "gain") : 1.0;
        string value = (amount * gain).ToString("G17", CultureInfo.InvariantCulture);
        return [new(input.Actor, $"Moved {value}")];
    }

    private static List<NameCommand> Signal(SchemaSnapshot input)
    {
        List<NameCommand> commands = [];
        foreach (SchemaNativeTarget target in input.Targets)
            commands.Add(new(target.Id, $"{target.Name} signalled"));
        return commands;
    }

    private static List<NameCommand> Label(SchemaSnapshot input) =>
        [new(input.Actor, Text(input, "text"))];

    private static double Number(SchemaSnapshot input, string field)
    {
        JsonElement value = input.Fields[field];
        JsonElement payload = value.GetProperty("value");
        if (value.GetProperty("type").GetString() != "f64" || payload.ValueKind != JsonValueKind.Number)
            throw new InvalidOperationException($"Expected f64 field '{field}'");
        return payload.GetDouble();
    }

    private static string Text(SchemaSnapshot input, string field)
    {
        JsonElement value = input.Fields[field];
        JsonElement payload = value.GetProperty("value");
        if (value.GetProperty("type").GetString() != "string" || payload.ValueKind != JsonValueKind.String)
            throw new InvalidOperationException($"Expected string field '{field}'");
        return payload.GetString()!;
    }
}
