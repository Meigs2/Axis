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
        var newBuffer = new byte[4];
        double result;
        try
        {
            Thread.Sleep(500);
            _controller.Write(29, PinValue.Low);
            _spiDevice.Read(newBuffer);
            _controller.Write(29, PinValue.High);
            result = ThermocoupleReading.Parse(newBuffer);
        }
        catch (Exception e)
        {
            Console.WriteLine(e);
            throw;
        }
        finally { ArrayPool<byte>.Shared.Return(buffer); }

        return result;
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
        uint raw = (uint)(bytes[3] << 24 | bytes[2] << 16 | bytes[1] << 8 | bytes[0]);

        bool fault = (raw & 0x00010000) != 0;

        // Calculate the fractional part
        int rawTemp = (int)(raw>>18);
        return rawTemp * 0.25;
    }
}