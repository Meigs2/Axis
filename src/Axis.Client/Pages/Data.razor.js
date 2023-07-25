// Declare the chart dimensions and margins.

let container = d3.select('.container')
// re-create something like this: https://codepen.io/browles/pen/mPMBjw
let h = container.node().clientHeight;
let w = container.node().clientWidth;

let time = 0;
let num = 300;

let noise = new SimplexNoise();
let seed = 50 + 100 * Math.random();
let data = [seed];
let averages_50 = [0];
let averages_25 = [0];
let deltas = [seed];

let latestData = [seed];
let latestAverages_50 = [0];
let latestAverages_25 = [0];
let latestDeltas = [seed];

let x = d3.scaleLinear().range([0, w - 40]);
let y = d3.scaleLinear().range([h - 40, 0]);

let xAxis = d3.axisBottom(x)
  .tickSizeInner(-h + 40)
  .tickSizeOuter(0)
  .tickPadding(10);

let yAxis = d3.axisLeft(y)
  .tickSizeInner(-w + 40)
  .tickSizeOuter(0)
  .tickPadding(10);

let line = d3.line()
  .x((d, i) => x(i + time - num))
  .y(d => y(d));

let svg = d3.select('#chart')
  .attr('width', w)
  .attr('height', h)
  .append('g')
  .attr('transform', 'translate(30, 20)');

let $xAxis = svg.append('g')
  .attr('class', 'x axis')
  .attr('transform', `translate(0, ${h - 40})`)
  .call(xAxis);

let $yAxis = svg.append('g')
  .attr('class', 'y axis')
  .call(yAxis);

let $data = svg.append('path')
  .attr('class', 'stroke-white fill-none stroke-2');

let $averages_50 = svg.append('path')
  .attr('class', 'stroke-white fill-none stroke-2');

let $averages_25 = svg.append('path')
  .attr('class', 'stroke-white fill-none stroke-2');

let $rects = svg.selectAll('rect')
  .data(d3.range(num))
  .enter()
    .append('rect')
    .attr('width', (w - 40) / num)
    .attr('x', (d, i) => i * (w - 40) / num);

let legend = svg.append('g')
  .attr('transform', `translate(20, 20)`)
  .selectAll('g')
  .data([['Value', '#fff'], ['Trailing Average - 50', '#0ff'], ['Trailing Average - 25', '#ff0']])
  .enter()
    .append('g');

legend
  .append('circle')
  .attr('fill', d => d[1])
  .attr('r', 5)
  .attr('cx', 0)
  .attr('cy', (d, i) => i * 15);

legend
  .append('text')
  .text(d => d[0])
  .attr('transform', (d, i) => `translate(10, ${i * 15 + 4})`);

function setDimensions() {
  let c = d3.select('.container').node()
  let w = c.clientWidth;
  let h = c.clientHeight;
  
  x = d3.scaleLinear().range([0, w - 40]);
  y = d3.scaleLinear().range([h - 40, 0]);

  xAxis = d3.axisBottom(x)
    .tickSizeInner(-h + 40)
    .tickSizeOuter(0)
    .tickPadding(10);

  yAxis = d3.axisLeft(y)
    .tickSizeInner(-w + 40)
    .tickSizeOuter(0)
    .tickPadding(10);
  
  // Update x-axis
  $xAxis.attr('transform', `translate(0, ${h - 40})`).call(xAxis);
  
  // Update vertical bars
  $rects.attr('width', (w - 40) / num)
        .attr('x', (d, i) => i * (w - 40) / num);

  line = d3.line()
    .x((d, i) => x(i + time - num))
    .y(d => y(d));
  
  let chart = d3.select('#chart')
  chart.attr("width", w);
  chart.attr("height", h);
  chart.attr("viewBox", `0 0 ${w} ${h}`);
}

// Call this function initially
setDimensions();

d3.select(window).on('resize', setDimensions);

function tick() {
  time++;
  data[time] = data[time - 1] + noise.noise2D(seed, time / 2);
  data[time] = Math.max(data[time], 0);

  if (time <= 50) {
    let a = 0;
    for (let j = 0; j < time; j++) {
      a += data[time - j];
    }
    a /= 50;
    averages_50[time] = a;
  }
  else {
    let a = averages_50[time - 1] * 50 - data[time - 50];
    a += data[time];
    a /= 50;
    averages_50[time] = a;
  }

  if (time <= 25) {
    let a = 0;
    for (let j = 0; j < time; j++) {
      a += data[time - j];
    }
    a /= 25;
    averages_25[time] = a;
  }
  else {
    let a = averages_25[time - 1] * 25 - data[time - 25];
    a += data[time];
    a /= 25;
    averages_25[time] = a;
  }

  deltas[time] = data[time] - data[time - 1];

  if (time <= num) {
    latestData = data.slice(-num);
    latestAverages_50 = averages_50.slice(-num);
    latestAverages_25 = averages_25.slice(-num);
    latestDeltas = deltas.slice(-num);
  }
  else {
    latestData.shift();
    latestAverages_50.shift();
    latestAverages_25.shift();
    latestDeltas.shift();
    latestData.push(data[time]);
    latestAverages_50.push(averages_50[time]);
    latestAverages_25.push(averages_25[time]);
    latestDeltas.push(deltas[time]);
  }
}

function update() {
  let container = d3.select('.container')
  // re-create something like this: https://codepen.io/browles/pen/mPMBjw
  let h = container.node().clientHeight;
  let w = container.node().clientWidth;
  
  x.domain([time - num, time]);
  let yDom = d3.extent(latestData);
  yDom[0] = Math.max(yDom[0] - 1, 0);
  yDom[1] += 1;
  y.domain(yDom);

  $xAxis
    .call(xAxis);

  $yAxis
    .call(yAxis);

  $data
    .datum(latestData)
    .attr('d', line);

  $averages_50
    .datum(latestAverages_50)
    .attr('d', line);

  $averages_25
    .datum(latestAverages_25)
    .attr('d', line);

  $rects
    .attr('height', (_, i) => Math.abs(latestDeltas[i] * h / 10))
    .attr('fill', (_, i) => latestDeltas[i] < 0 ? 'red' : 'green')
    .attr('y', (_, i) => h - Math.abs(latestDeltas[i] * h / 10) - 42);
}

for (let i = 0; i < num + 50; i++) {
  tick();
}

update();

setInterval(() => {
  tick();
  update();
}, 60);