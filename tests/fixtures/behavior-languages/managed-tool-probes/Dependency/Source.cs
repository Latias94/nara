namespace Nara.SourceToolProbes;

public static class ProbeDependency
{
    // A property call keeps the dependency reference in the entry assembly's IL.
    public static int Value =>
#if DEPENDENCY_REVISION_TWO
        2;
#else
        1;
#endif
}
