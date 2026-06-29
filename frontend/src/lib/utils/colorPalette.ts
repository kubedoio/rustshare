export interface ColorOption {
	key: string;
	label: string;
	bgClass: string;
	borderClass: string;
	editorClass: string;
}

export const COLOR_PALETTE: ColorOption[] = [
	{
		key: 'pink',
		label: 'Pink',
		bgClass: 'bg-pink-500',
		borderClass: 'border-l-pink-500',
		editorClass: 'bg-[var(--rs-accent-pink)]'
	},
	{
		key: 'red',
		label: 'Red',
		bgClass: 'bg-red-500',
		borderClass: 'border-l-red-500',
		editorClass: 'bg-[var(--rs-accent-red)]'
	},
	{
		key: 'orange',
		label: 'Orange',
		bgClass: 'bg-orange-500',
		borderClass: 'border-l-orange-500',
		editorClass: 'bg-[var(--rs-accent-orange)]'
	},
	{
		key: 'yellow',
		label: 'Yellow',
		bgClass: 'bg-yellow-500',
		borderClass: 'border-l-yellow-500',
		editorClass: 'bg-[var(--rs-accent-yellow)]'
	},
	{
		key: 'green',
		label: 'Green',
		bgClass: 'bg-green-500',
		borderClass: 'border-l-green-500',
		editorClass: 'bg-[var(--rs-accent-green)]'
	},
	{
		key: 'blue',
		label: 'Blue',
		bgClass: 'bg-blue-500',
		borderClass: 'border-l-blue-500',
		editorClass: 'bg-[var(--rs-accent-blue)]'
	},
	{
		key: 'purple',
		label: 'Purple',
		bgClass: 'bg-purple-500',
		borderClass: 'border-l-purple-500',
		editorClass: 'bg-[var(--rs-accent-purple)]'
	},
	{
		key: 'gray',
		label: 'Gray',
		bgClass: 'bg-gray-500',
		borderClass: 'border-l-gray-500',
		editorClass: 'bg-[var(--rs-accent-gray)]'
	}
];

export function getColorOption(key: string | null | undefined): ColorOption | undefined {
	return COLOR_PALETTE.find((c) => c.key === key);
}
