namespace Twip;

public partial class Bytes
{
    /// <summary>The bytes as a new array.</summary>
    public byte[] ToArray()
    {
        var a = new byte[(int)Len];
        CopyTo(a);
        return a;
    }
}
