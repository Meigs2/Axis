namespace Axis.Core;

public abstract class AxisTask
{
    public abstract void Run(AxisApplication app, DateTime currentTime);
}

public class ScheduledTasks
{
    private readonly List<AxisTask> _tasks = new List<AxisTask>();

    public void Add(AxisTask task)
    {
        _tasks.Add(task);
    }
}