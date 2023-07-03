// See https://aka.ms/new-console-template for more information

using Axis.Core;

Console.WriteLine("Hello, World!");

var builder = new AxisApplicationBuilder().ConfigureServices(s => { });
builder.AddTask<GetBoilerTemperature>();

var app = builder.Build();

app.Run();