using Axis.Core.Messaging;
using Microsoft.Extensions.DependencyInjection;

namespace Axis.Core;

public class AxisApplication
{
    public IServiceProvider Services { get; }
    
    public AxisApplication(IServiceProvider services)
    {
        Services = services;
    }
}

public class AxisApplicationBuilder
{
    private readonly IServiceCollection _services = new ServiceCollection();

    public AxisApplicationBuilder ConfigureServices(Action<IServiceCollection> configureServices)
    {
        configureServices(_services);
        _services.AddSingleton<AxisApplication>();
        _services.AddSingleton<Rp2040>();

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
        return services;
    }
}