class D3MultiAxisChart {
    constructor(containerSelector) {
        this.svg = null;
        this.xAxisGroup = null;
        this.xScale = null;
        this.yAxes = {};
        this.containerSelector = containerSelector;

        this.initializeChart();
    }

    initializeChart() {
        const container = d3.select(this.containerSelector).node();
        this.width = container.clientWidth;
        this.height = container.clientHeight;

        this.svg = d3.select(container).append('svg')
            .attr('width', this.width)
            .attr('height', this.height);

        this.xScale = d3.scaleLinear().range([0, this.width - 40]);

        const xAxis = d3.axisBottom(this.xScale)
            .tickSizeInner(-this.height + 40)
            .tickSizeOuter(0)
            .tickPadding(10);

        this.xAxisGroup = this.svg.append('g')
            .attr('class', 'x axis')
            .attr('transform', `translate(0, ${this.height - 40})`)
            .call(xAxis);
    }

    addYAxis(name, range, position) {
        const yScale = d3.scaleLinear().range([this.height - 40, 0]).domain(range);

        const yAxis = d3.axisLeft(yScale)
            .tickSizeInner(-this.width + 40)
            .tickSizeOuter(0)
            .tickPadding(10);

        const yAxisGroup = this.svg.append('g')
            .attr('class', `${name} axis`)
            .attr('transform', `translate(${position}, 0)`)
            .call(yAxis);

        this.yAxes[name] = {
            scale: yScale,
            axis: yAxis,
            group: yAxisGroup,
            data: []
        };

        return this.yAxes[name];
    }

    updateData(axisName, dataPoint) {
        const axis = this.yAxes[axisName];
        if (!axis) {
            throw new Error(`Axis "${axisName}" does not exist`);
        }

        axis.data.push(dataPoint);
        this.updateXAxis();
        this.updateYAxis(axis);
    }

    updateXAxis() {
        const allData = Object.values(this.yAxes).flatMap(axis => axis.data);
        const maxTime = Math.max(...allData.map(d => d.time));

        this.xScale.domain([0, maxTime]);
        this.xAxisGroup.call(d3.axisBottom(this.xScale));
    }

    updateYAxis(axis) {
        const line = d3.line()
            .x(d => this.xScale(d.time))
            .y(d => axis.scale(d.value));

        axis.group.append('path')
            .datum(axis.data)
            .attr('class', 'line')
            .attr('d', line);
    }
}

const noiseGenerator = new SimplexNoise();
let time = 0;

// Initialize chart and axes
const chart = new D3MultiAxisChart('.container');
const axis1 = chart.addYAxis('value', [-50, 50], 40);
const axis2 = chart.addYAxis('average', [-50, 50], 80);

// Timer tick function
setInterval(() => {
    time++;

    const noise1 = noiseGenerator.noise2D(0, time);
    const noise2 = noiseGenerator.noise2D(0, time / 2);

    chart.updateData('value', { time, value: noise1 * 50 });
    chart.updateData('average', { time, value: noise2 * 50 });
}, 20);
