namespace Axis.Core;
using System;

public class PidController
{
    private double _kP;
    private double _kI;
    private double _kD;

    private double _integral = 0.0;
    private double _lastProportional = 0.0;
    
    private double _integralLimit = 100.0;

    public PidController(double kP, double kI, double kD)
    {
        _kP = kP;
        _kI = kI;
        _kD = kD;
    }
    
    public PidController(double kP, double kI, double kD, double integralLimit)
    {
        _kP = kP;
        _kI = kI;
        _kD = kD;
        _integralLimit = integralLimit;
    }

    public double Update(double setpoint, double actual, TimeSpan timeFrame)
    {
        // Calculate error
        double proportional = setpoint - actual;

        // Calculate integral with timeframe as seconds
        _integral += proportional * timeFrame.TotalMilliseconds;

        // Limit integral
        if (_integral > _integralLimit)
        {
            _integral = _integralLimit;
        }
        else if (_integral < -_integralLimit)
        {
            _integral = -_integralLimit;
        }

        // Calculate derivative with timeframe as seconds
        double derivative = (proportional - _lastProportional) / timeFrame.TotalSeconds;

        // Calculate output
        double output = _kP * proportional + _kI * _integral + _kD * derivative;

        // Save proportional error for next calculation
        _lastProportional = proportional;

        return output;
    }
}
