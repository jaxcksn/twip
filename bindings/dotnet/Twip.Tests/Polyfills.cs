#if NETFRAMEWORK
// .NET Framework 4.8 lacks these newer APIs the tests call (Twip.Core's Polyfills has the shared ones).
namespace Twip.Tests;

internal static class TestPolyfills
{
    extension(Type type)
    {
        public bool IsAssignableTo(Type target) => target.IsAssignableFrom(type);
    }

    extension(Enum)
    {
        public static T[] GetValues<T>() where T : struct, Enum => (T[])Enum.GetValues(typeof(T));

        public static T Parse<T>(string value) where T : struct, Enum => (T)Enum.Parse(typeof(T), value);
    }

    extension<T>(IEnumerable<T> items)
    {
        public IOrderedEnumerable<T> Order() => items.OrderBy(x => x);
    }

    extension<TKey, TValue>(Dictionary<TKey, TValue> dictionary) where TKey : notnull
    {
        public TValue GetValueOrDefault(TKey key, TValue fallback) => dictionary.TryGetValue(key, out var value) ? value : fallback;
    }

    extension(string s)
    {
        public bool StartsWith(char c) => s.Length > 0 && s[0] == c;
    }

    extension(char)
    {
        public static bool IsAsciiLetter(char c) => c is >= 'a' and <= 'z' or >= 'A' and <= 'Z';

        public static bool IsAsciiDigit(char c) => c is >= '0' and <= '9';
    }

    extension(File)
    {
        public static Task<string> ReadAllTextAsync(string path, CancellationToken ct = default) => Task.Run(() => File.ReadAllText(path), ct);

        public static Task WriteAllTextAsync(string path, string text, CancellationToken ct = default) =>
            Task.Run(() => File.WriteAllText(path, text), ct);
    }

    extension(Directory)
    {
        public static DirectoryInfo CreateTempSubdirectory(string prefix) =>
            Directory.CreateDirectory(Path.Combine(Path.GetTempPath(), prefix + Path.GetRandomFileName()));
    }

    extension(Task task)
    {
        public async Task WaitAsync(TimeSpan timeout, CancellationToken ct)
        {
            if (await Task.WhenAny(task, Task.Delay(timeout, ct)).ConfigureAwait(false) != task) throw new TimeoutException();
            await task.ConfigureAwait(false);
        }
    }

    extension<T>(Task<T> task)
    {
        public async Task<T> WaitAsync(TimeSpan timeout, CancellationToken ct)
        {
            if (await Task.WhenAny(task, Task.Delay(timeout, ct)).ConfigureAwait(false) != task) throw new TimeoutException();
            return await task.ConfigureAwait(false);
        }
    }
}
#endif
