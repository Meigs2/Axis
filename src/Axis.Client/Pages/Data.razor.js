// re-create something like this: https://codepen.io/browles/pen/mPMBjw
let c = d3.select('.container').node()
let w = c.clientWidth;
let h = c.clientHeight;

let time = 0;
let num = 300;

let noise = new SimplexNoise();
let seed = 0;
let data = [seed];

let x = d3.scaleLinear().range([0, w - 40]);
let y = d3.scaleLinear().range([h - 40, 0]);
let y2 = d3.scaleLinear().range([h - 40, 0]);

let xAxis = d3.axisBottom(x)
    .tickSizeInner(-h + 40)
    .tickSizeOuter(0)
    .tickPadding(10);

let yAxis = d3.axisLeft(y)
    .tickSizeInner(-w + 40)
    .tickSizeOuter(0)
    .tickPadding(10);
    
let yAxis2 = d3.axisLeft(y2)
    .tickSizeInner(-w + 40)
    .tickSizeOuter(0)
    .tickPadding(10);

let line = d3.line()
    .x((d, i) => x(i))
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
    
let $yAxis2 = svg.append('g')
    .attr('class', 'y axis')
    .attr('transform', `translate(${+40}, 0)`)
    .call(yAxis2);

let $data = svg.append('path')
    .attr('class', 'stroke-white fill-none stroke-2');

let legend = svg.append('g')
    .attr('transform', `translate(20, 20)`)
    .selectAll('g')
    .data([
        ['Value', '#fff'],
        ['Trailing Average - 50', '#0ff'],
        ['Trailing Average - 25', '#ff0']
    ])
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
    y2 = d3.scaleLinear().range([h - 40, 0]);

    xAxis = d3.axisBottom(x)
        .tickSizeInner(-h + 40)
        .tickSizeOuter(0)
        .tickPadding(10);

    yAxis = d3.axisLeft(y)
        .tickSizeInner(-w + 40)
        .tickSizeOuter(0)
        .tickPadding(10);
        
    yAxis2 = d3.axisLeft(y2)
        .tickSizeInner(-w + 40)
        .tickSizeOuter(0)
        .tickPadding(10);

    // Update x-axis
    $xAxis.attr('transform', `translate(0, ${h - 40})`);

    line = d3.line()
        .x((d, i) => x(i))
        .y(d => y(d));

    let chart = d3.select('#chart')
    chart.attr("width", w);
    chart.attr("height", h);
    chart.attr("viewBox", `0 0 ${w} ${h}`);

    update();
}

// Call this function initially
setDimensions();

d3.select(window).on('resize', setDimensions);

function tick() {
    time++;
    data[time] = data[time - 1] + noise.noise2D(seed, time);
}

function update() {
    let c = d3.select('.container').node()
    let w = c.parentElement.clientWidth;
    let h = c.parentElement.clientHeight;
    let extent = d3.extent(data);
    
    if (time <= 300) {
        x.domain([0, 300]);
    } else {
        x.domain([0, time]);
    }

    y.domain([Math.min(0, extent[0]), Math.max(50, extent[1])]);
    y2.domain([Math.min(0, extent[0]), Math.max(50, extent[1])]);

    $xAxis
        .call(xAxis);

    $yAxis
        .call(yAxis);
    
    $yAxis2
    	.call(yAxis2)

    line
        .x((d, i) => x(i))
        .y(d => y(d));

    $data
        .datum(data)
        .attr('d', line);
}

update();

setInterval(() => {
    tick();
    update();
}, 20);
