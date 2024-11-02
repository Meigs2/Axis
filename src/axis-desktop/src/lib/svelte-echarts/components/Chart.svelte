<script lang="ts">
  import type {
    init as baseInit,
    EChartsType as BaseEchartsType,
    EChartsOption,
    SetOptionOpts,
  } from 'echarts'
  import type { init as coreInit, EChartsType as CoreEchartsType } from 'echarts/core'
  import type { EChartsInitOpts } from 'echarts'
  import { createEventDispatcher, onMount } from 'svelte'
  import { EVENT_NAMES, type EventHandlers } from '$lib/svelte-echarts/constants/events'

  interface Props {
    init: typeof baseInit | typeof coreInit;
    theme?: string | object | null;
    initOptions?: EChartsInitOpts;
    options: EChartsOption;
    notMerge?: SetOptionOpts['notMerge'];
    lazyUpdate?: SetOptionOpts['lazyUpdate'];
    silent?: SetOptionOpts['silent'];
    replaceMerge?: SetOptionOpts['replaceMerge'];
    transition?: SetOptionOpts['transition'];
    chart?: (BaseEchartsType | CoreEchartsType) | undefined;
    [key: string]: any
  }

  let {
    init,
    theme = 'light',
    initOptions = {},
    options,
    notMerge = true,
    lazyUpdate = false,
    silent = false,
    replaceMerge = undefined,
    transition = undefined,
    chart = $bindable(undefined),
    ...rest
  }: Props = $props();

  let element = $state<HTMLDivElement | undefined>();

  const dispatch = createEventDispatcher<EventHandlers>()

  const initChart = () => {
    if (chart) chart.dispose()

    chart = init(element, theme, initOptions)

    EVENT_NAMES.forEach((eventName) => {
      // @ts-expect-error
      chart!.on(eventName, (event) => dispatch(eventName, event))
    })
  }

	const init_chart: Action = (node) => {
		// the node has been mounted in the DOM
		$effect(() => {
			// setup goes here
      if (chart) chart.setOption(options, { notMerge, lazyUpdate, silent, replaceMerge, transition })

			return () => {
				// teardown goes here
			};
		});
	};

  onMount(() => {
    const resizeObserver = new ResizeObserver(() => {
      if (!chart) initChart()
      else chart.resize()
    })

    window.addEventListener('resize', () => {chart?.resize()});

    resizeObserver.observe(element!)

    return () => {
      resizeObserver.disconnect()
      chart?.dispose()
    }
  })
</script>

<!-- restProps is currently broken with typescript -->

<div style="width: 100%; height: 100%;" bind:this={element} use:init_chart {...rest}></div>
