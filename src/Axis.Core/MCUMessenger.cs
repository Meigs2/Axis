using System.Buffers;
using System.Device.Gpio;
using System.Device.Spi;
using System.IO.Ports;
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

    public static Message Deserialize(Span<byte> bytes)
    {
        var contentLength = BitConverter.ToUInt16(bytes[2..4]);
        return new Message()
        {
            MessageType = (MessageType)bytes[0],
            ContentLength = contentLength,
            Content = contentLength > 0 ? bytes[5..contentLength].ToArray() : Array.Empty<byte>()
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

public class MicroController : IDisposable
{
    private Subject<Message> _subject = new();
    public IObservable<Message> Observable => _subject.AsObservable();
    private SerialPort _serialPort;

    public MicroController()
    {
        _serialPort = new SerialPort("/dev/cs.usbmodem123456781");
    }
    
    public void Send(Message message)
    {
        try
        {
            var buffer = message.Seralize();
            _serialPort.Write(buffer.ToArray(), 0, buffer.Length);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
    }

    public async void ReadMessages()
    {
        while (true)
        {
            try
            {
                if (!_serialPort.IsOpen)
                {
                    _serialPort.Open();
                }
                
                var messageType = (ushort)_serialPort.ReadByte();
                var buff = new byte[2];
                _serialPort.Read(buff, 0, 2);
                var contentLength = BitConverter.ToUInt16(buff);
                buff = new byte[contentLength];
                var content = _serialPort.Read(buff, 0, contentLength);

                _subject.Publish(
                    new Message((MessageType)messageType) { ContentLength = contentLength, Content = buff });
            }
            catch (Exception e)
            {
                Console.WriteLine(e);
                throw;
            }
        }
    }

    public void Dispose()
    {
        _subject.Dispose();
    }
}