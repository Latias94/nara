using System.Globalization;
using System.Text.Json;
using Nara.BehaviorTrial;

namespace Nara.AuthoredBehaviors;

public sealed partial class Module : IBehaviorModule
{
    private readonly Dictionary<string, Func<SchemaSnapshot, List<NameCommand>>> callbacks =
        new(StringComparer.Ordinal)
        {
            ["trial.behavior.Mover"] = Move,
#if !REVISION_MISSING
            ["trial.behavior.Sensor"] = Signal,
#endif
            ["trial.behavior.Label"] = Label,
        };

    public SchemaBinding[] Describe() => callbacks.Keys
        .OrderBy(schema => schema, StringComparer.Ordinal)
#if REVISION_INVALID
        .Select(schema => new SchemaBinding(schema, schema == "trial.behavior.Sensor" ? 2u : 1u))
#else
        .Select(schema => new SchemaBinding(schema, 1))
#endif
        .ToArray();

    public List<NameCommand> Invoke(SchemaSnapshot input)
    {
        if (input.Fault == "throw")
            throw new InvalidOperationException("Trial callback failure");
        if (!callbacks.TryGetValue(input.Schema, out var callback))
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
        double result = amount * gain;
#if REVISION_TWO
        result *= 2;
#endif
        string value = result.ToString("G17", CultureInfo.InvariantCulture);
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
