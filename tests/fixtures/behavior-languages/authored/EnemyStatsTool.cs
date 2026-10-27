using System.Globalization;
using System.Text.Json;
using Nara.BehaviorTrial;

namespace Nara.AuthoredBehaviors;

public sealed partial class Module : IEnemyStatsTool
{
    public JsonElement DescribeEnemyStats()
    {
        string declaration = """
            {
              "kind": "component_schema_catalog",
              "format_version": 1,
              "engine_min_version": "0.1.0",
              "generator": { "name": "nara", "version": "0.1.0" },
              "payload": {
                "generation": 1,
                "components": [
                  {
                    "id": "trial.enemy.EnemyStats",
                    "aliases": ["EnemyStats"],
                    "version": 1,
                    "capabilities": ["scene", "inspect", "edit"],
                    "fields": [
                      {
                        "id": "health",
                        "aliases": ["Health"],
                        "path": [{ "kind": "field", "value": "wire_health" }],
                        "shape": { "shape": "scalar", "kind": "i64" },
                        "required": true,
                        "capabilities": ["scene", "inspect", "edit"],
                        "member_tombstones": []
                      },
                      {
                        "id": "armor",
                        "aliases": ["Armor"],
                        "path": [{ "kind": "field", "value": "wire_armor" }],
                        "shape": { "shape": "scalar", "kind": "i64" },
                        "required": true,
                        "capabilities": ["scene", "inspect", "edit"],
                        "member_tombstones": []
                      }
                    ],
                    "field_tombstones": [],
                    "authoring_initial_value": {
                      "type": "map",
                      "value": {
                        "wire_health": { "type": "i64", "value": 100 },
                        "wire_armor": { "type": "i64", "value": 10 }
                      }
                    }
                  }
                ],
                "type_tombstones": []
              }
            }
            """;
#if REVISION_INVALID
        declaration = declaration.Replace("\"version\": 1,", "\"version\": 2,", StringComparison.Ordinal);
#endif
        using JsonDocument catalog = JsonDocument.Parse(declaration);
        return catalog.RootElement.Clone();
    }

    public EnemyStatsProposal ProposeEnemyStats(EnemyStatsRequest request)
    {
        if (request.Fault == "throw")
            throw new InvalidOperationException("Trial EnemyStats tool failure");
        string title = request.Operation switch
        {
            "attach" => $"Attach EnemyStats to {request.Name}",
            "edit" => $"Edit EnemyStats for {request.Name}",
            "spawn" => $"Spawn {request.Name} with EnemyStats",
            _ => throw new InvalidOperationException("Unknown EnemyStats operation"),
        };
        long bonus = request.Bonus;
#if REVISION_TWO
        bonus = checked(bonus * 2);
#endif
        long health = checked(request.Health + bonus);
        List<EnemyStatsWrite> writes =
        [
            new("health", EnemyStatsInteger(health)),
            new("armor", EnemyStatsInteger(request.Armor)),
        ];
        if (request.Fault == "bad_last_value")
        {
            using JsonDocument invalid = JsonDocument.Parse("""{"type":"string","value":"invalid armor"}""");
            writes[1] = new("armor", invalid.RootElement.Clone());
        }
        if (request.Fault == "too_many_writes")
        {
            while (writes.Count < 9)
                writes.Add(writes[0]);
        }
        return new EnemyStatsProposal(title, request.Name, writes.ToArray());
    }

    private static JsonElement EnemyStatsInteger(long value)
    {
        using JsonDocument document = JsonDocument.Parse(
            $$"""{"type":"i64","value":{{value.ToString(CultureInfo.InvariantCulture)}}}""");
        return document.RootElement.Clone();
    }
}
