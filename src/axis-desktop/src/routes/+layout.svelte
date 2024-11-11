<script>
  import "../app.css";
  import AppSidebar from "$lib/components/app-sidebar.svelte";
  import * as Sidebar from "$lib/components/ui/sidebar";
  import Separator from "$lib/components/ui/separator/separator.svelte";
  import * as Breadcrumb from "$lib/components/ui/breadcrumb"
  import { ModeWatcher } from "mode-watcher";
  import LightSwitch from "$lib/components/light-switch.svelte";


  let { children } = $props();
 
</script>

<Sidebar.Provider>
	<AppSidebar collapsible='icon' />
	<Sidebar.Inset>
		<header class="flex h-16 shrink-0 items-center gap-2 border-b px- justify-between">
			<Sidebar.Trigger class="ml-4" />
      <LightSwitch class="mr-4"/>
		</header>
    <main class="flex h-full w-full">
      {@render children()}
    </main>
	</Sidebar.Inset>
</Sidebar.Provider>