const ctx = document.getElementById('myChart');
let data = [];
let labels = []; // Add this line to create an array for your labels

let dataSet = {
      borderWidth: 2,
      radius: 0,
      data: data,
};

const config = {
  type: 'line',
  data: {
    labels: labels, // Add this line to set the labels in your chart data
    datasets: [
    	dataSet
    ]
  },
  options: {
  maintainAspectRatio: false,
  animation: false,
    interaction: {
      intersect: false
    },
    plugins: {
      legend: false
    },
    scales: {
      x: {
        type: 'linear',
        suggestedMin: 0,
        suggestedMax: 30
      },
     y: {
        type: 'linear',
        suggestedMin: 0,
        suggestedMax: 5
      }
    }
  }
};

let chart = new Chart(ctx, config);
let ticks = 0;
let previous = Date.now();
export function addData(value) {
    let current = Date.now();
    ticks += current - previous;
    previous = current;
    chart.data.datasets.forEach((dataset) => {
        dataset.data.push(value);
    });
    labels.push(ticks/1000); // Add this line to update the labels
    chart.update();
}
