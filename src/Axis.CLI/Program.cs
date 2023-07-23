// See https://aka.ms/new-console-template for more information

using System.Reactive.Linq;
using Axis.Core;
using Microsoft.Extensions.DependencyInjection;

Console.WriteLine("Hello, World!");

var builder = new AxisApplicationBuilder().ConfigureServices(s => { });

var app = builder.Build();

var mcu = app.Services.GetRequiredService<MasterControlUnit>();

mcu.Observable.Subscribe(x => Console.WriteLine(x.ToString()));

var task = Task.Run(() =>
{
    while (true)
    {
        mcu.ReadMessages();
    }
});

while (true)
{
    Console.WriteLine("Press Key to send message");
    Console.ReadKey();

    mcu.Send(new Message.Ping());
    mcu.Send(new Message.Pong());
}