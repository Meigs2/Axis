// See https://aka.ms/new-console-template for more information

using System.Reactive.Linq;
using Axis.Core;
using Microsoft.Extensions.DependencyInjection;

Console.WriteLine("Hello, World!");

var builder = new AxisApplicationBuilder().ConfigureServices(s => { });

var app = builder.Build();

var mcu = app.Services.GetRequiredService<MasterControlUnit>();

var task = Task.Run(async () =>
{
    while (true)
    {
        await mcu.ReadMessages();
    }
});

Console.WriteLine("Press Key to send message");
Console.ReadKey();

while (true)
{
    mcu.Send(new Message.Ping());
    Console.ReadKey();
}