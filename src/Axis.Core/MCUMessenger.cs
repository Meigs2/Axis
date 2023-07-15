using System.Buffers;
using System.Device.Gpio;
using System.Device.Spi;
using System.IO.Ports;
using System.Reactive.Linq;
using System.Reactive.Subjects;

using System.Formats.Cbor;
using Dahomey.Cbor;
using Dahomey.Cbor.ObjectModel;
using Dahomey.Cbor.Util;

namespace Axis.Core;

public struct MessageDTO
{
    public MessageType MessageType { get; set; }
    public UInt16 ContentLength { get; set; }
    public byte[] Content { get; set; }

    public MessageDTO(MessageType type)
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

    public static MessageDTO Deserialize(Span<byte> bytes)
    {
        var contentLength = BitConverter.ToUInt16(bytes[2..4]);
        return new MessageDTO()
        {
            MessageType = (MessageType)bytes[0],
            ContentLength = contentLength,
            Content = contentLength > 0 ? bytes[5..contentLength].ToArray() : Array.Empty<byte>()
        };
    }

    public override string ToString()
    {
        return "Message!";
    }
}

public enum MessageType : UInt16
{
    Startup = 0,
    Acknowledge = 1,
    Ping = 2,
    Pong = 3,
    ThermocoupleReading = 4,
}

public class MicroController : IDisposable
{
    public Subject<MessageDTO> _subject = new();
    public IObservable<MessageDTO> Observable => _subject.AsObservable();
    private SerialPort _serialPort;

    public MicroController()
    {
        _serialPort = new SerialPort("/dev/tty.usbmodem123456781");
    }
    
    public async Task Send(MessageDTO messageDto)
    {
        if (!_serialPort.IsOpen)
        {
            _serialPort.Open();
        }

        var buffer = new byte[1028];
        var byteStream = new MemoryStream();
        try
        {
            var bufferWriter = new ByteBufferWriter();
            await Cbor.SerializeAsync(messageDto, byteStream);
            _serialPort.Write(byteStream.ToArray(), 0, (int)byteStream.Length);
            Thread.Sleep(10);
            ReadMessage();
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
            ReadMessage();
        }
    }

    private void ReadMessage()
    {

        var buffer = new byte[1028];
        try
        {
            if (!_serialPort.IsOpen)
            {
                _serialPort.Open();
            }

            _serialPort.Read(buffer, 0, 64);
            var result = Cbor.Deserialize<MessageDTO>(buffer);
            Console.WriteLine("MCU Reply: " + result);
            _subject.Publish(result);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
    }

    public void Dispose()
    {
        _subject.Dispose();
    }
}