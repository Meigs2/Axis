using System.Buffers;
using System.Device.Gpio;
using System.Device.Spi;
using System.Reactive.Linq;
using System.Reactive.Subjects;

namespace Axis.Core;

public struct Message
{
    public MessageType MessageType { get; set; }
    public ushort ContentLength { get; set; }
    public byte[] Content { get; set; }

    public Message(MessageType type)
    {
        MessageType = type;
        ContentLength = 0;
        Content = Array.Empty<byte>();
    }

    public Span<byte> Seralize()
    {
        var bytes = new List<byte>();
        var t = (byte)MessageType;
        var len = BitConverter.GetBytes(ContentLength);
        var content = Content;
        bytes.Add(t);
        bytes.AddRange(len);
        bytes.AddRange(content);

        return bytes.ToArray().AsSpan();
    }

    public Message Deserialize(Span<byte> bytes)
    {
        return new Message()
        {
            MessageType = (MessageType)bytes[0],
            ContentLength = BitConverter.ToUInt16(bytes[2..4]),
            Content = ContentLength > 0 ? bytes[5..ContentLength].ToArray() : Array.Empty<byte>()
        };
    }
}

public enum MessageType : byte
{
    Startup = 0,
    Acknowledge = 1,
    Ping = 2,
    Pong = 3,
    ThermocoupleReading = 4,
}

public class MicroController
{
    private readonly GpioController _controller;
    private Subject<Message> _subject = new();
    private SpiDevice _spiDevice;
    public IObservable<Message> Observable => _subject.AsObservable();

    public MicroController(GpioController controller)
    {
        _controller = controller;
        Initialize();
    }
    
    public void Initialize()
    {
        _spiDevice = SpiDevice.Create(new SpiConnectionSettings(0, 0) { ClockFrequency = 1_000_000});
        _controller.OpenPin(3, PinMode.Output);
    }

    public void Send(Message message)
    {
        try
        {
            _controller.Write(3, PinValue.High);
            _spiDevice.Write(message.Seralize());
            _controller.Write(3, PinValue.Low);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
    }

    public async void ReadMessages()
    {
        
    }
}