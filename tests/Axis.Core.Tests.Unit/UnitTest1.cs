using Axis.Core.Peripherals;
using FluentAssertions;

namespace Axis.Core.Tests.Unit;

public class UnitTest1
{
    [Fact]
    public void BoilerTemperature_Should_ReturnCorrectTemperature()
    {
        var rawData = 0b_0110_0100_0000_0000;
        var str = Convert.ToString(rawData, 2);
        double temperature = BoilerThermocouple.CalculateTemperature(rawData);
        temperature.Should().Be(1600.00);
    }
}