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
			rustshare: {
				'primary': '#ef6f28',
				'primary-content': '#ffffff',
				'secondary': '#3b82f6',
				'secondary-content': '#ffffff',
				'accent': '#10b981',
				'accent-content': '#ffffff',
				'neutral': '#374151',
				'neutral-content': '#f3f4f6',
				'base-100': '#0f1115',
				'base-200': '#181b21',
				'base-300': '#21262d',
				'base-content': '#f0f2f5',
				'info': '#3b82f6',
				'success': '#10b981',
				'warning': '#f59e0b',
				'error': '#ef4444',
				'--rounded-btn': '0.5rem',
				'--rounded-box': '0.75rem',
				'--rounded-badge': '0.375rem',
			}
		}
	],
	darkTheme: 'rustshare',
	base: true,
	styled: true,
	utils: true,
}
};
