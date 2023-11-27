using System.IO.Ports;
using System.Reactive.Linq;
using System.Reactive.Subjects;
using System.Reflection;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Axis.Core.Messaging;

public abstract class Message
{
    public static JsonSerializerSettings ConverterSettings = new()
    {
        Converters = new List<JsonConverter>() { new MessageConverter() }
    };

    public override string ToString()
    {
        return JsonConvert.SerializeObject(this, Message.ConverterSettings);
    }
}

public class Ping : Message
{
}

public class Pong : Message
{
    public string value { get; init; } = string.Empty;
}

public class ThermocoupleReading : Message
{
    public double value { get; init; } = 0.0;
}

public class AdsReading : Message
{
    public double value { get; init; } = 0.0;
}

public class BrewSwitch
{
    public bool is_on { get; init; }
}

public interface IMicrocontroller
{
    IObservable<AdsReading> AdsReadouts { get; }
    IObservable<ThermocoupleReading> ThermocoupleReadings { get; }
    IObservable<Message> Messages { get; }
    Task ReadMessagesLoop();
}

public class DemoMicrocontroller : IMicrocontroller
{
    public IObservable<AdsReading> AdsReadouts =>
        Observable.Generate(initialState: 3.0,
                            condition: _ => true, // Always true, so it keeps generating values indefinitely
                            iterate: value =>
                                value + (Random.Shared.NextDouble() - 0.5) * 2, // Increment the value in each tick
                            resultSelector:
                            value => new AdsReading() { value = value }, // Select the value to be emitted
                            timeSelector: _ => TimeSpan.FromMilliseconds(30) // Set the tick interval to 30 milliseconds
        );

    public IObservable<ThermocoupleReading> ThermocoupleReadings =>
        Observable.Generate(initialState: 75.0,
                            condition: _ => true, // Always true, so it keeps generating values indefinitely
                            iterate: value =>
                                value + (Random.Shared.NextDouble() - 0.5) * 2, // Increment the value in each tick
                            resultSelector:
                            value => new ThermocoupleReading() { value = value }, // Select the value to be emitted
                            timeSelector: _ => TimeSpan.FromMilliseconds(30) // Set the tick interval to 30 milliseconds
        );

    public IObservable<Message> Messages => Observable.Never<Message>();

    public Task ReadMessagesLoop()
    {
        return Task.Delay(TimeSpan.MaxValue);
    }
}

public class Rp2040 : IDisposable, IMicrocontroller
{
    private Subject<Message> _subject = new();
    public IObservable<Message> Messages => _subject.AsObservable();
    public IObservable<AdsReading> AdsReadouts => _subject.OfType<AdsReading>();
    public IObservable<ThermocoupleReading> ThermocoupleReadings => _subject.OfType<ThermocoupleReading>();
    private SerialPort _serialPort;

    public Rp2040()
    {
        _serialPort = new SerialPort("/dev/tty.usbmodem123456781");
        var options = new JsonSerializerSettings();
        options.Converters.Add(new MessageConverter());
    }

    public void Send(Message message)
    {
        if (!_serialPort.IsOpen) { _serialPort.Open(); }

        try
        {
            var json = JsonConvert.SerializeObject(message, Message.ConverterSettings);
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

    public Task ReadMessages()
    {
        try
        {
            if (!_serialPort.IsOpen) { _serialPort.Open(); }

            var result = _serialPort.ReadLine();
            Console.WriteLine(result);
            if (result.Length == 0) return Task.CompletedTask;
            var message = JsonConvert.DeserializeObject<Message>(result, Message.ConverterSettings);
            _subject.OnNext(message);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }

        return Task.CompletedTask;
    }

    public Task ReadMessagesLoop() =>
        Task.Run(async () =>
        {
            while (true) { await ReadMessages(); }
        });

    public void Dispose()
    {
        _subject.Dispose();
    }
}

public class MessageConverter : JsonConverter
{
    public static Dictionary<string, Type> MessageTypes = typeof(Message).Assembly.GetTypes()
                                                                         .Where(x => x.IsAssignableTo(typeof(Message)))
                                                                         .ToDictionary(x => x.Name);

    public override bool CanConvert(Type objectType)
    {
        return typeof(Message).IsAssignableFrom(objectType);
    }

    public override object ReadJson(JsonReader reader, Type objectType, object existingValue, JsonSerializer serializer)
    {
        var item = JObject.Load(reader);
        var token = item.First;

        if (MessageTypes.ContainsKey(token.Path))
        {
            return JsonConvert.DeserializeObject(token.First.ToString(), MessageTypes[token.Path]);
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