<script>
  import "../app.css";
  import AppSidebar from "$lib/components/app-sidebar.svelte";
  import * as Sidebar from "$lib/components/ui/sidebar";
  import Separator from "$lib/components/ui/separator/separator.svelte";
  import * as Breadcrumb from "$lib/components/ui/breadcrumb"

  let { children } = $props();
 
</script>

<Sidebar.Provider>
	<AppSidebar />
	<Sidebar.Inset>
		<header class="flex h-16 shrink-0 items-center gap-2 border-b px-4">
			<Sidebar.Trigger class="-ml-1" />
		</header>
    <main class="flex h-full w-full">
      {@render children()}
    </main>
	</Sidebar.Inset>
</Sidebar.Provider>