using System.Buffers;

namespace Axis.Core;

public struct Message
{
    public MessageType MessageType { get; set; }
    public uint ByteLength { get; set; }
    public byte[] StringBytes { get; set; }

    public Span<byte> Seralize()
    {
        ArrayPool<>
    }
}

public enum MessageType : 
{
    Event = 0,
    Request = 1,
    Response = 2,
}

public class MCUMessenger
{
}