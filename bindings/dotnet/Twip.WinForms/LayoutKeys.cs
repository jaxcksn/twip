using System.Runtime.InteropServices;

namespace Twip.WinForms;

/// <summary>What the keyboard layout types for a key.</summary>
internal static class LayoutKeys
{
    [ThreadStatic] private static byte[]? t_state;
    [ThreadStatic] private static char[]? t_typed;

    /// <summary>Whether the key down in <paramref name="msg"/>, with the modifiers held now, types a character (or
    /// starts one, a dead key) on the thread's keyboard layout: AltGr, or Ctrl+Alt, on a layout with one for the key.</summary>
    public static bool TypesCharacter(in Message msg)
    {
        var state = t_state ??= new byte[256];
        var typed = t_typed ??= new char[4];
        if (!GetKeyboardState(state)) return false;
        var vk = (uint)(nint)msg.WParam;
        var scan = (uint)(((long)(nint)msg.LParam >> 16) & 0xFF);
        // 4: the keyboard state (a pending dead key) stays as it was, from Windows 10 1607.
        var n = ToUnicodeEx(vk, scan, state, typed, typed.Length, 4, GetKeyboardLayout(0));
        return n < 0 || (n > 0 && !char.IsControl(typed[0]));
    }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetKeyboardState(byte[] state);

    [DllImport("user32.dll")]
    private static extern nint GetKeyboardLayout(uint thread);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int ToUnicodeEx(uint key, uint scan, byte[] state, [Out] char[] buffer, int size, uint flags, nint layout);
}
