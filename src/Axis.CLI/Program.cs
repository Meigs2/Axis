// See https://aka.ms/new-console-template for more information

using System.Reactive.Linq;
using Axis.Core;
using Microsoft.Extensions.DependencyInjection;

Console.WriteLine("Hello, World!");

var builder = new AxisApplicationBuilder().ConfigureServices(s => { });

var app = builder.Build();

var mcu = app.Services.GetRequiredService<MicroController>();

Console.WriteLine("Press any to to perform handshake...\n");
Console.ReadKey();

mcu.Observable.Subscribe(x => Console.WriteLine(x.ToString()));
mcu._subject.Publish(new MessageDTO());

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

await mcu.Send(new MessageDTO(MessageType.Startup){} );

app.Run();