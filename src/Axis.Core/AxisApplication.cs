using System;
using System.Collections.Generic;
using System.Device.Gpio;
using System.Reflection;
using Axis.Core.Peripherals;
using Microsoft.Extensions.DependencyInjection;

namespace Axis.Core;

public struct ScheduledTask : IComparable<ScheduledTask>
{
    public Action Action { get; private set; }
    public ExecuteAt ScheduledTime { get; private set; }

    public ScheduledTask(Action action, DateTime scheduledTime)
    {
        Action = action;
        ScheduledTime = scheduledTime;
    }

    public ScheduledTask(Action action, DateTime scheduledTime, int priority)
    {
        Action = action;
        ScheduledTime = new ExecuteAt(scheduledTime, priority);
    }

    public int CompareTo(ScheduledTask other)
    {
        var compare = ScheduledTime.CompareTo(other.ScheduledTime);
        if (compare == 0)
        {
            // If ScheduledTime is the same, compare by priority
            compare = ScheduledTime.CompareTo(other.ScheduledTime);
        }

        return compare;
    }
}

public struct ExecuteAt : IComparable<ExecuteAt>
{
    public ExecuteAt(DateTime dateTime, int priority)
    {
        Priority = priority;
        ScheduledTime = dateTime;
    }

    public int Priority { get; }
    public DateTime ScheduledTime { get; }

    // add conversion from DateTime to ExecuteAt with priority 0
    public static implicit operator ExecuteAt(DateTime dateTime) { return new ExecuteAt(dateTime, 0); }

    public int CompareTo(ExecuteAt other)
    {
        var priorityComparison = Priority.CompareTo(other.Priority);
        if (priorityComparison != 0) return priorityComparison;
        return ScheduledTime.CompareTo(other.ScheduledTime);
    }
}

public class AxisApplication
{
    private List<AxisTask> _tasks;
    private PriorityQueue<ScheduledTask, ExecuteAt> _eventQueue;
    CancellationTokenSource _cts = new();
    private SpinWait _spinWait;
    
    public AxisApplication(List<AxisTask> tasks)
    {
        _tasks = tasks;
        _eventQueue = new PriorityQueue<ScheduledTask, ExecuteAt>();
    }

    public void ScheduleEvent(Action action, DateTime scheduledTime, int priority)
    {
        var element = new ScheduledTask(action, scheduledTime, priority);
        _eventQueue.Enqueue(element, element.ScheduledTime);
    }

    public void Run()
    {
        while (!_cts.IsCancellationRequested)
        {
            ExecuteTasks();
            ExecuteEvents();
        }
    }

    private void ExecuteTasks()
    {
        for (int i = 0; i < _tasks.Count; i++)
        {
            _tasks[i].Run(this, DateTime.Now);
        }
    }

    private void ExecuteEvents()
    {
        if (_eventQueue.Count == 0)
        {
            _spinWait.SpinOnce(1000);
            return;
        }

        if (!_eventQueue.TryPeek(out var nextEvent, out var nextEventTime)) return;

        if (nextEventTime.ScheduledTime <= DateTime.Now)
        {
            try
            {
                nextEvent.Action();
                _eventQueue.Dequeue();
            }
            catch (Exception e)
            {
                Console.WriteLine(e);
                throw;
            }
        }
        else
        {
            _spinWait.SpinOnce(100);
        }
    }
}

public class AxisApplicationBuilder
{
    private readonly List<AxisTask> _tasks = new();
    private readonly IServiceCollection _services = new ServiceCollection();

    public AxisApplicationBuilder ConfigureServices(Action<IServiceCollection> configureServices)
    {
        configureServices(_services);
        _services.AddSingleton<AxisApplication>();
        _services.AddSingleton<ScheduledTasks>();
        _services.AddSingleton(new GpioController(PinNumberingScheme.Board));
        _services.AddSingleton<BoilerThermocouple>();
        _services.AddSingleton<List<AxisTask>>(s => s.GetServices<AxisTask>().ToList());

        return this;
    }

    public AxisApplicationBuilder AddTask<TTask>() where TTask : AxisTask
    {
        _services.AddSingleton<AxisTask, TTask>();
        return this;
    }

    public AxisApplication Build()
    {
        return _services.BuildServiceProvider().GetRequiredService<AxisApplication>();
    }
}

public static class AxisApplicationExtensions
{
    public static IServiceCollection AddAxis(this IServiceCollection services, Action<IServiceCollection> configureServices)
    {
        configureServices(services);
        services.AddSingleton<AxisApplication>();
        services.AddSingleton<ScheduledTasks>();
        return services;
    }
}