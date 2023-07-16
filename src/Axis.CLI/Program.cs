// See https://aka.ms/new-console-template for more information

using System.Reactive.Linq;
using Axis.Core;
using Microsoft.Extensions.DependencyInjection;

Console.WriteLine("Hello, World!");

var builder = new AxisApplicationBuilder().ConfigureServices(s => { });

var app = builder.Build();

var mcu = app.Services.GetRequiredService<MicroController>();

mcu.Observable.Subscribe(x => Console.WriteLine(x.ToString()));

while (true)
{
    Console.WriteLine("Press Key to send message");
    Console.ReadKey();

    await mcu.Send(new Message.Ping());
    await mcu.Send(new Message.Pong());
}

// Task.Run(() =>
// {
//     while (true)
//     {
//         try
//         {
//             //mcu.ReadMessages();
//         }
//         catch (Exception e)
//         {
//             Console.WriteLine(e);
//         }
//     }
// });

app.Run();