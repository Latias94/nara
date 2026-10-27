namespace Nara.SourceToolProbes;

internal static class Program
{
#if PROBE_ASYNC
    private static async Task<int> Main(string[] args)
    {
        await Task.Yield();
        Console.WriteLine($"managed-probe:{ProbeDependency.Value}");
        return 0;
    }
#else
#if PROBE_STA
    [STAThread]
#endif
    private static int Main(string[] args)
    {
        Console.WriteLine($"managed-probe:{ProbeDependency.Value}");
        return 0;
    }
#endif
}
