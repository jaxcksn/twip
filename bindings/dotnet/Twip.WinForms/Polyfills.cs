#if NETFRAMEWORK
// .NET Framework 4.8 lacks these newer APIs: the same calls, on top of what it has (Twip.Core's Polyfills has the
// shared ones).
using System.Globalization;

namespace Twip.WinForms;

internal static class Polyfills
{
    extension(Math)
    {
        public static int Clamp(int value, int min, int max) => value < min ? min : value > max ? max : value;
    }

    extension(char)
    {
        public static bool IsAsciiDigit(char c) => c is >= '0' and <= '9';
    }

    extension(int)
    {
        public static bool TryParse(ReadOnlySpan<char> s, IFormatProvider provider, out int result) =>
            int.TryParse(s.ToString(), NumberStyles.Integer, provider, out result);
    }

    extension(Clipboard)
    {
        public static bool TryGetData<T>(string format, out T? data)
        {
            if (Clipboard.GetData(format) is T value)
            {
                data = value;
                return true;
            }
            data = default;
            return false;
        }
    }

    extension<TKey, TValue>(Dictionary<TKey, TValue> dictionary) where TKey : notnull
    {
        public bool Remove(TKey key, out TValue value) => dictionary.TryGetValue(key, out value!) && dictionary.Remove(key);
    }
}
#endif
