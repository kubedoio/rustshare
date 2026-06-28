export interface ColorOption {
	key: string;
	label: string;
	bgClass: string;
	borderClass: string;
}

export const COLOR_PALETTE: ColorOption[] = [
	{ key: 'red', label: 'Red', bgClass: 'bg-red-500', borderClass: 'border-l-red-500' },
	{ key: 'orange', label: 'Orange', bgClass: 'bg-orange-500', borderClass: 'border-l-orange-500' },
	{ key: 'yellow', label: 'Yellow', bgClass: 'bg-yellow-500', borderClass: 'border-l-yellow-500' },
	{ key: 'green', label: 'Green', bgClass: 'bg-green-500', borderClass: 'border-l-green-500' },
	{ key: 'blue', label: 'Blue', bgClass: 'bg-blue-500', borderClass: 'border-l-blue-500' },
	{ key: 'purple', label: 'Purple', bgClass: 'bg-purple-500', borderClass: 'border-l-purple-500' },
	{ key: 'gray', label: 'Gray', bgClass: 'bg-gray-500', borderClass: 'border-l-gray-500' }
];

export function getColorOption(key: string | null | undefined): ColorOption | undefined {
	return COLOR_PALETTE.find((c) => c.key === key);
}
