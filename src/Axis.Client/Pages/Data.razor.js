// Declare the chart dimensions and margins.

let chartInstance;

// re-create something like this: https://codepen.io/browles/pen/mPMBjw

export function createChartInstance() {
    chartInstance = new Data();
    return true;
}

export function appendDataToChart(time, temperature) {
    if (chartInstance) {
        chartInstance.appendData(time, temperature);
    }
}

export function initChart() {
    if (chartInstance) {
        chartInstance.init();
    }
}

const width = 640;
const height = 400;
const marginTop = 20;
const marginRight = 20;
const marginBottom = 30;
const marginLeft = 40;

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

let svg = d3.select('body').append('svg')
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
  .attr('class', 'line data');

// Append the SVG element.
export class Data {
    
    init() {
        // Declare the x (horizontal position) scale.
        this.x = d3.scaleLinear()
            .domain(0, 30000)
            .range([marginLeft, width - marginRight]);

        // Declare the y (vertical position) scale.
        this.y = d3.scaleLinear()
            .domain([0, 100])
            .range([height - marginBottom, marginTop]);

        // Create the SVG container.
        this.svg = d3.create("svg")
            .attr("width", width)
            .attr("height", height)

        // Add the x-axis.
        this.xAxis = this.svg.append("g")
            .attr("transform", `translate(0,${height - marginBottom})`)
            .call(d3.axisBottom(this.x));

        // Add the y-axis.
        this.yAxis = this.svg.append("g")
            .attr("transform", `translate(${marginLeft},0)`)
            .call(d3.axisLeft(this.y));
        
        this.line = d3.line()
            .x(d => this.x(d.time))
            .y(d => this.y(d.temperature));

        this.data = [];
        
        d3.select('.container').node().appendChild(this.svg.node());
        this.$data = this.svg.append("path")
            .attr("fill", "none")
            .attr("stroke", "steelblue")
            .attr("stroke-width", 1.5)
            .attr("d", this.line(this.data));
    }

    appendData(time, temperature) {
        this.data.push({time, temperature});
        this.$data
            .datum(this.data)
            .attr('d', this.line);
        
        // If data exceeds the time window, remove the oldest data point
        // if (this.data[0].time < time - 30000) {
        //     this.data.shift();
        // }
    }
}
