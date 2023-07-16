using System.IO.Ports;
using System.Reactive.Linq;
using System.Reactive.Subjects;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Axis.Core;

public struct MessageDTO
{
    public MessageType message_type { get; set; }
    public string contents { get; set; } = "Okay";

    public MessageDTO(MessageType type)
    {
        message_type = type;
    }

    public override string ToString()
    {
        return JsonSerializer.Serialize(this);
    }
}

public abstract class Message
{
    public class Ping : Message
}

public enum MessageType : UInt16
{
    Startup ,
    Acknowledge ,
    Ping = 2,
    Pong = 3,
    ThermocoupleReading = 4,
}

public class MicroController : IDisposable
{
    public Subject<MessageDTO> _subject = new();
    public IObservable<MessageDTO> Observable => _subject.AsObservable();
    private SerialPort _serialPort;
    
    JsonSerializerOptions _options = new()
    {
        Converters ={
            new JsonStringEnumConverter()
        }
    };
    
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

        try
        {
            var json = JsonSerializer.Serialize(messageDto, _options);
            var bytes = Encoding.UTF8.GetBytes(json);
            Console.WriteLine(json);
            _serialPort.Write(bytes, 0, bytes.Length);
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
        var buffer = new byte[64];
        try
        {
            
            if (!_serialPort.IsOpen)
            {
                _serialPort.Open();
            }

            var result = _serialPort.ReadExisting();
            Console.WriteLine("MCU Reply: " + result);
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