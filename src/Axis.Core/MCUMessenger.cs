using System.IO.Ports;
using System;
using System.Reactive.Linq;
using System.Reactive.Subjects;
using System.Threading;
using System.Reflection;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using JsonConverter = Newtonsoft.Json.JsonConverter;
using JsonSerializer = Newtonsoft.Json.JsonSerializer;

namespace Axis.Core;

public abstract class Message
{
    public static JsonSerializerSettings ConverterSettings = new()
    {
        Converters = new List<JsonConverter>() { new MessageConverter() }
    };
    
    public class Ping : Message
    {
    }

    public class Pong : Message
    {
        public string value { get; set; } = string.Empty;
    }

    public class ThermocoupleReading : Message
    {
        public double temperature { get; set; } = 0.0;
    }

    public class AdsReading : Message
    {
        public double value { get; set; } = 0.0;
    }

    public override string ToString()
    {
        return JsonConvert.SerializeObject(this, Message.ConverterSettings);
    }
}

public interface IMasterControlUnit
{
    IObservable<Message.AdsReading> AdsReadouts { get; }
}

public class FakeDataMasterControlUnit : IMasterControlUnit
{
    public IObservable<Message.AdsReading> AdsReadouts => Observable.Generate(
        initialState: 3.0,
        condition: value => true, // Always true, so it keeps generating values indefinitely
        iterate: value => value + (Random.Shared.NextDouble() - 0.5) * 2, // Increment the value in each tick
        resultSelector: value => new Message.AdsReading() { value = value }, // Select the value to be emitted
        timeSelector: value => TimeSpan.FromMilliseconds(30) // Set the tick interval to 30 milliseconds
    );
}

public class MasterControlUnit : IDisposable, IMasterControlUnit
{
    private Subject<Message> _subject = new();
    public IObservable<Message.AdsReading> AdsReadouts => _subject.OfType<Message.AdsReading>();

    private SerialPort _serialPort;
    private JsonSerializerSettings _options;

    public MasterControlUnit(string serialPortName)
    {
        _serialPort = new SerialPort(serialPortName);
        _options = new JsonSerializerSettings();
        _options.Converters.Add(new MessageConverter());
    }

    public void Send(Message message)
    {
        if (!_serialPort.IsOpen)
        {
            _serialPort.Open();
        }
        try
        {
            var messages = new List<Message>(1){message};
            var json = JsonConvert.SerializeObject(messages, Message.ConverterSettings);
            var bytes = Encoding.UTF8.GetBytes(json);
            Console.WriteLine(json);
            _serialPort.Write(bytes, 0, bytes.Length);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
    }

    public async void ReadMessages()
    {
        try
        {
            Thread.Sleep(1);
            if (!_serialPort.IsOpen) { _serialPort.Open(); }

            var result = _serialPort.ReadExisting();
            if (result.Length == 0)
            {
                return;
            }
            var messages = JsonConvert.DeserializeObject<IEnumerable<Message>>(result, Message.ConverterSettings);
            foreach (var message in messages!)
            {
                _subject.OnNext(message);
            }
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
        var item = JObject.Load(reader);
        var token = item.First;

        switch (token.Path)
        {
            case "Pong":
                return JsonConvert.DeserializeObject<Message.Ping>(token.First.ToString());
            case "ThermocoupleReading":
                return JsonConvert.DeserializeObject<Message.ThermocoupleReading>(token.First.ToString());
            case "AdsReading":
                return JsonConvert.DeserializeObject<Message.AdsReading>(token.First.ToString());
        }
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