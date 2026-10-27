using System.Collections.ObjectModel;
using System.Text.Json;

namespace Nara.BehaviorTrial;

public sealed record NameCommand(string Target, string Value);
public sealed record SchemaNativeTarget(string Id, string Name);
public sealed record SchemaBinding(string Schema, uint Version);

public sealed record SchemaSnapshot
{
    public string Schema { get; }
    public string Actor { get; }
    public IReadOnlyDictionary<string, JsonElement> Fields { get; }
    public IReadOnlyList<SchemaNativeTarget> Targets { get; }
    public string Fault { get; }

    public SchemaSnapshot(
        string schema,
        string actor,
        IReadOnlyDictionary<string, JsonElement> fields,
        IReadOnlyList<SchemaNativeTarget> targets,
        string fault)
    {
        Schema = schema;
        Actor = actor;
        Fields = new ReadOnlyDictionary<string, JsonElement>(fields.ToDictionary(
            field => field.Key, field => field.Value.Clone(), StringComparer.Ordinal));
        Targets = Array.AsReadOnly(targets.ToArray());
        Fault = fault;
    }
}

public interface IBehaviorModule
{
    SchemaBinding[] Describe();
    List<NameCommand> Invoke(SchemaSnapshot snapshot);
}

public sealed record EnemyStatsRequest(
    string Operation,
    string Name,
    long Health,
    long Armor,
    long Bonus,
    string Fault);
public sealed record EnemyStatsWrite(string Field, JsonElement Value);
public sealed record EnemyStatsProposal(string Title, string Name, EnemyStatsWrite[] Writes);

public interface IEnemyStatsTool
{
    JsonElement DescribeEnemyStats();
    EnemyStatsProposal ProposeEnemyStats(EnemyStatsRequest request);
}
