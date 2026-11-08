#if NETFRAMEWORK
// .NET Framework 4.8 lacks these newer APIs: the same calls, on top of what it has. C# 14 extension members, so the
// call sites read as they do on .NET.
using System.Runtime.CompilerServices;

namespace Twip;

internal static class Polyfills
{
    extension(ArgumentNullException)
    {
        public static void ThrowIfNull(object? argument, [CallerArgumentExpression(nameof(argument))] string? paramName = null)
        {
            if (argument is null) throw new ArgumentNullException(paramName);
        }
    }

    extension(ObjectDisposedException)
    {
        public static void ThrowIf(bool condition, object instance)
        {
            if (condition) throw new ObjectDisposedException(instance.GetType().FullName);
        }
    }

    extension(Math)
    {
        public static double Clamp(double value, double min, double max) => value < min ? min : value > max ? max : value;
    }

    extension(double)
    {
        public static bool IsFinite(double d) => !double.IsNaN(d) && !double.IsInfinity(d);
    }

    extension(File)
    {
        public static Task<byte[]> ReadAllBytesAsync(string path, CancellationToken ct = default) =>
            Task.Run(() => File.ReadAllBytes(path), ct);

        public static Task WriteAllBytesAsync(string path, byte[] bytes, CancellationToken ct = default) =>
            Task.Run(() => File.WriteAllBytes(path, bytes), ct);

        public static void Move(string source, string destination, bool overwrite)
        {
            if (overwrite && File.Exists(destination)) File.Replace(source, destination, null);
            else File.Move(source, destination);
        }
    }

    extension(ThreadPool)
    {
        public static bool UnsafeQueueUserWorkItem<T>(Action<T> callBack, T state, bool preferLocal) =>
            ThreadPool.UnsafeQueueUserWorkItem(s => callBack((T)s!), state);
    }

    extension(Stream stream)
    {
        public Task CopyToAsync(Stream destination, CancellationToken ct) => stream.CopyToAsync(destination, 81920, ct);

        public Task WriteAsync(byte[] buffer, CancellationToken ct) => stream.WriteAsync(buffer, 0, buffer.Length, ct);
    }

    extension(string s)
    {
        /// <summary>Only ordinal, the only comparison the SDK replaces with.</summary>
        public string Replace(string oldValue, string newValue, StringComparison comparison) =>
            comparison == StringComparison.Ordinal ? s.Replace(oldValue, newValue) : throw new NotSupportedException();
    }

    extension(HttpContent content)
    {
        public Task<string> ReadAsStringAsync(CancellationToken ct) => content.ReadAsStringAsync();
    }

    public static void Deconstruct<TKey, TValue>(this KeyValuePair<TKey, TValue> pair, out TKey key, out TValue value) =>
        (key, value) = (pair.Key, pair.Value);
}
#endif
