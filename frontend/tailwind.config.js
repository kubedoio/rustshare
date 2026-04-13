/** @type {import('tailwindcss').Config} */
export default {
	content: ['./src/**/*.{html,js,svelte,ts}'],
	theme: {
		extend: {
			colors: {
				brand: {
					50: '#fff4ec',
					100: '#f8e2d2',
					200: '#efc4a8',
					300: '#e29b74',
					400: '#d47648',
					500: '#c65a1e',
					600: '#a34716',
					700: '#833a15',
					800: '#683016',
					900: '#562916',
					950: '#2e1409'
				}
			},
			fontFamily: {
				sans: ['Instrument Sans', 'system-ui', 'sans-serif'],
				display: ['Fraunces', 'Georgia', 'serif'],
				data: ['IBM Plex Sans', 'system-ui', 'sans-serif'],
				mono: ['IBM Plex Mono', 'monospace']
			},
			fontSize: {
				'2xs': ['0.625rem', { lineHeight: '0.875rem' }],
				'meta': ['0.75rem', { lineHeight: '1.35', fontWeight: '500' }],
				'body-sm': ['0.875rem', { lineHeight: '1.45', fontWeight: '400' }]
			},
			boxShadow: {
				panel: '0 18px 45px rgba(39, 25, 14, 0.08)',
				'panel-dark': '0 22px 54px rgba(0, 0, 0, 0.24)'
			},
			borderRadius: {
				// Design system hierarchy: sm(6px) md(10px) lg(14px) xl(20px) pill(999px)
				'sm': '0.375rem',    // 6px
				'md': '0.625rem',    // 10px
				'lg': '0.875rem',    // 14px
				'xl': '1.25rem',     // 20px
				'2xl': '1.5rem',     // 24px - for cards/panels
				'3xl': '2rem'        // 32px - for large containers
			}
		}
	}
};
