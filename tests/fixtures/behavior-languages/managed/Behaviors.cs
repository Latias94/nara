using System.Globalization;

namespace Nara.BehaviorTrial;

public sealed record NativeTarget(string Id, string Name);
public sealed record Snapshot(string Event, string Actor, double Amount, NativeTarget[] Targets, string Fault);
public sealed record NameCommand(string Target, string Value);

public static class Behaviors
{
    public static List<NameCommand> Invoke(Snapshot input)
    {
        if (input.Fault == "throw")
            throw new InvalidOperationException("Trial callback failure");
        List<NameCommand> commands = [];
        if (input.Event == "tick")
        {
            commands.Add(new(input.Actor, $"Moved {input.Amount.ToString("G17", CultureInfo.InvariantCulture)}"));
        }
        else if (input.Event == "signal")
        {
            foreach (NativeTarget target in input.Targets)
                commands.Add(new(target.Id, $"{target.Name} signalled"));
        }
        else
        {
            throw new InvalidOperationException("Unknown event");
        }
        if (input.Fault == "bad_target")
            commands.Add(new("unavailable", "bad"));
        return commands;
    }
}
