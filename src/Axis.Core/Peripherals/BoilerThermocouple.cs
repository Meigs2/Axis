using System.Buffers;
using System.Device.Gpio;
using System.Device.Gpio.Drivers;
using System.Device.Spi;

namespace Axis.Core.Peripherals;

public class BoilerThermocouple
{
    private readonly GpioController _controller;
    private SpiDevice _spiDevice;

    public BoilerThermocouple(GpioController controller)
    {
        _controller = controller;
        Initialize();
    }

    public void Initialize()
    {
        _spiDevice = SpiDevice.Create(new SpiConnectionSettings(0, 0));
        _controller.OpenPin(29, PinMode.Output);
    }

    public double Read()
    {
        var buffer = ArrayPool<byte>.Shared.Rent(4);
        double result;
        try
        {
            Thread.Sleep(500);
            _controller.Write(29, PinValue.Low);
            _spiDevice.Read(buffer);
            result = Parse(buffer[..4]);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(buffer);
            _controller.Write(29, PinValue.High);
        }

        return result;
    }

    public static double Parse(Span<byte> bytes)
    {
        if (bytes.Length != 4) { throw new ArgumentException("Input bytes length must be 4", nameof(bytes)); }

        // Calculate the fractional part
        int rawTemp = bytes[0] << 24 | (bytes[1] >> 2) << 16;
        
        var str = string.Format($"0b{BitConverter.ToString(bytes.ToArray())}");
        
        return CalculateTemperature(rawTemp);
    }

    public static double CalculateTemperature(int rawData)
    {
        int maxBits = 14; // maximum bits for the temperature data
        double precision = 100.0; // precision is 2 decimal points
        int rawTemperature = rawData & ((1 << maxBits) - 1); // get the lower 14 bits of the raw data
        // Check if the number is negative
        if ((rawTemperature & (1 << (maxBits - 1))) != 0)
        {
            // If the number is negative, we apply a bitwise NOT operation, add 1 to result, then multiply by -1
            rawTemperature = -((~rawTemperature & ((1 << maxBits) - 1)) + 1);
        }

        // Finally, we adjust for the decimal point and return the result
        return rawTemperature / precision;
    }
}

public static class ThermocoupleReading
{
}