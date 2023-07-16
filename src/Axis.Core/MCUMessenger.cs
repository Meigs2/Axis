using System.IO.Ports;
using System.Reactive.Linq;
using System.Reactive.Subjects;
using System.Reflection;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using JsonConverter = Newtonsoft.Json.JsonConverter;
using JsonSerializer = Newtonsoft.Json.JsonSerializer;

namespace Axis.Core;

public abstract class Message
{
    public class Ping : Message
    {
    }

    public class Pong : Message
    {
        public string value { get; set; } = string.Empty;
    }
}

public class MicroController : IDisposable
{
    public Subject<Message> _subject = new();
    public IObservable<Message> Observable => _subject.AsObservable();
    private SerialPort _serialPort;
    private JsonSerializerSettings _options;

    public MicroController()
    {
        _serialPort = new SerialPort("/dev/tty.usbmodem123456781");
        _options = new JsonSerializerSettings();
        _options.Converters.Add(new MessageConverter());
    }

    public async Task Send(Message message)
    {
        if (!_serialPort.IsOpen)
        {
            _serialPort.Open();
        }
        try
        {
            var json = JsonConvert.SerializeObject(message, _options);
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
        while (true) { ReadMessage(); }
    }

    private void ReadMessage()
    {
        try
        {
            if (!_serialPort.IsOpen) { _serialPort.Open(); }

            var result = _serialPort.ReadExisting();
            var message = JsonConvert.DeserializeObject<Message>(result, _options);
            Console.WriteLine("MCU Reply: " + JsonConvert.SerializeObject(message, _options));
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
    }

    public void Dispose() { _subject.Dispose(); }
}

public class MessageConverter : JsonConverter
{
    public override bool CanConvert(Type objectType) { return typeof(Message).IsAssignableFrom(objectType); }

    public override object ReadJson(JsonReader reader, Type objectType, object existingValue, JsonSerializer serializer)
    {
        JObject item = JObject.Load(reader);
        var token = item.First;
        var type = Assembly.GetExecutingAssembly()
                           .GetTypes()
                           .FirstOrDefault(t => t.Name.Equals(token.Path, StringComparison.OrdinalIgnoreCase));
        if (type == null) { throw new ArgumentException($"No matching type found for '{token.Path}'."); }

        return token.First.ToObject(type);
    }

    public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
    {
        var type = value.GetType();
        var properties = type.GetProperties(BindingFlags.Public | BindingFlags.Instance);
        if (properties.Length == 0)
        {
            writer.WriteValue(type.Name);
            return;
        }
        else
        {
            writer.WriteStartObject();
            writer.WritePropertyName(type.Name);
            writer.WriteStartObject();
            foreach (var propertyInfo in properties)
            {
                writer.WritePropertyName(propertyInfo.Name);
                serializer.Serialize(writer, propertyInfo.GetValue(value));
            }
            writer.WriteEndObject();
            writer.WriteEndObject();
        }
    }
}