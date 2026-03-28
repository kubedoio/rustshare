/** @type {import('tailwindcss').Config} */
export default {
	content: ['./src/**/*.{html,js,svelte,ts}'],
	theme: {
		extend: {
		colors: {
			// RustShare brand colors
			brand: {
				50: '#fef7f0',
				100: '#fdecd9',
				200: '#fbd4b3',
				300: '#f7b580',
				400: '#f28f4d',
				500: '#ef6f28',
				600: '#e1561e',
				700: '#bb4119',
				800: '#95351b',
				900: '#782e19',
				950: '#411509',
			}
		},
		fontFamily: {
			sans: ['Inter', 'system-ui', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI', 'Roboto', 'sans-serif'],
		},
		fontSize: {
			'2xs': ['0.625rem', { lineHeight: '0.875rem' }],
		}
	}
},
plugins: [require('daisyui')],
	daisyui: {
		themes: [
			{
				'rustshare-light': {
					'primary': '#ef6f28',
					'primary-content': '#fffaf5',
					'secondary': '#c55b24',
					'secondary-content': '#fffaf5',
					'accent': '#d97706',
					'accent-content': '#fffaf5',
					'neutral': '#2a211d',
					'neutral-content': '#fffaf5',
					'base-100': '#fcfaf7',
					'base-200': '#f4efe7',
					'base-300': '#e6ddcf',
					'base-content': '#251d17',
					'info': '#ef6f28',
					'success': '#1f9d68',
					'warning': '#c17b11',
					'error': '#d1495b',
					'--rounded-btn': '0.75rem',
					'--rounded-box': '1rem',
					'--rounded-badge': '0.5rem',
				}
			},
			{
				'rustshare-dark': {
					'primary': '#ef6f28',
					'primary-content': '#ffffff',
					'secondary': '#f28f4d',
					'secondary-content': '#ffffff',
					'accent': '#d97706',
					'accent-content': '#ffffff',
					'neutral': '#374151',
					'neutral-content': '#f3f4f6',
					'base-100': '#0f1115',
					'base-200': '#181b21',
					'base-300': '#21262d',
					'base-content': '#f0f2f5',
					'info': '#f28f4d',
					'success': '#10b981',
					'warning': '#f59e0b',
					'error': '#ef4444',
					'--rounded-btn': '0.75rem',
					'--rounded-box': '1rem',
					'--rounded-badge': '0.5rem',
				}
			}
		],
		darkTheme: 'rustshare-dark',
		base: true,
		styled: true,
		utils: true,
}
};
