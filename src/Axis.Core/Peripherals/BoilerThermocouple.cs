using System.Buffers;
using System.Device.Gpio;
using System.Device.Gpio.Drivers;
using System.Device.Spi;

namespace Axis.Core.Peripherals;

public class BoilerThermocouple
{
    private SpiDevice _spiDevice;

    public void Initialize()
    {
        var controller = new GpioController();
        
        _spiDevice = SpiDevice.Create(new SpiConnectionSettings(0, 0)
        {
            ClockFrequency = 1_000_000, Mode = SpiMode.Mode0, DataBitLength = 14,
        });
    }

    public double Read()
    {
        var buffer = ArrayPool<byte>.Shared.Rent(4);
        try
        {
            _spiDevice.Read(buffer[..4]);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
        finally { ArrayPool<byte>.Shared.Return(buffer); }

        return ThermocoupleReading.Parse(buffer);
    }
}

public static class ThermocoupleReading
{
    private const int FractionalBits = 2;
    private const int TemperatureBits = 14;
    private const int SignBitMask = 1 << (TemperatureBits - 1);
    private const double FractionalStep = 0.25;

    public static double Parse(Span<byte> bytes)
    {
        if (bytes.Length != 4) { throw new ArgumentException("Input bytes length must be 4", nameof(bytes)); }

        // Combine the four bytes into a 32-bit value
        int raw = bytes[3] << 24 | bytes[2] << 16 | bytes[1] << 8 | bytes[0];

        // Shift right to align the temperature data to the least significant bits
        raw >>= 18;

        // If the sign bit is set, it's a negative number
        if ((raw & SignBitMask) != 0)
        {
            // Two's complement to get the absolute value
            raw = -(~(raw - 1) & (SignBitMask - 1));
        }

        // Calculate the fractional part
        int fractional = raw & ((1 << FractionalBits) - 1);
        double fractionalValue = fractional * FractionalStep;

        // Combine the integer and fractional parts
        return (raw >> FractionalBits) + fractionalValue;
    }
}