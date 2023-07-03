namespace Axis.Core;

public class GetBoilerTemperature : AxisTask
{
    public override void Run(AxisApplication app, DateTime currentTime)
    {
        Console.WriteLine(Random.Shared.NextDouble());
    }
}