// See Polyfills.cs.
#if NETFRAMEWORK
namespace System.Threading.Tasks;

/// <summary>The non-generic TaskCompletionSource .NET 5 added.</summary>
internal sealed class TaskCompletionSource(TaskCreationOptions options)
{
    private readonly TaskCompletionSource<bool> _inner = new(options);

    public Task Task => _inner.Task;

    public void SetResult() => _inner.SetResult(true);

    public bool TrySetResult() => _inner.TrySetResult(true);

    public void SetException(Exception exception) => _inner.SetException(exception);
}
#endif
