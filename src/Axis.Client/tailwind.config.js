module.exports = {
    content: ['./**/*.{razor,html}'],
    darkMode: 'class',
    theme: {
        extend: {
            colors: {
                // Colors are based on the following site: https://realtimecolors.com/?colors=e2e0f5-0a091b-f97316-1e1b4b-6d28d9
                background: {
                    DEFAULT: '#0A091B',
                },
                primary: 'orange-500',
                secondary: 'indigo-950',
                focus: 'violet-700',
            }
        },
    },
    plugins: [],
}
