using Axis.Core.Peripherals;

namespace Axis.Core;

public class GetBoilerTemperature : AxisTask
{
    private readonly BoilerThermocouple _couple;

    public GetBoilerTemperature(BoilerThermocouple couple)
    {
        _couple = couple;
    }

    public override void Run(AxisApplication app, DateTime currentTime)
    {
        Console.WriteLine(_couple.Read());
    }
}