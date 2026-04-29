export interface CropSelection {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface ImageDimensions {
	width: number;
	height: number;
}

export class ImageEditor {
	private canvas: HTMLCanvasElement;
	private ctx: CanvasRenderingContext2D;
	private history: ImageData[] = [];
	private historyIndex = -1;
	private maxHistorySize = 20;
	private originalImage: HTMLImageElement | null = null;

	constructor(canvas: HTMLCanvasElement) {
		this.canvas = canvas;
		const ctx = canvas.getContext('2d');
		if (!ctx) {
			throw new Error('Could not get 2D context from canvas');
		}
		this.ctx = ctx;
	}

	async loadImage(src: string): Promise<void> {
		return new Promise((resolve, reject) => {
			const img = new Image();
			img.crossOrigin = 'anonymous';
			img.onload = () => {
				this.originalImage = img;
				this.canvas.width = img.width;
				this.canvas.height = img.height;
				this.ctx.drawImage(img, 0, 0);
				this.saveState();
				resolve();
			};
			img.onerror = reject;
			img.src = src;
		});
	}

	loadFromFile(file: File): Promise<void> {
		return new Promise((resolve, reject) => {
			const reader = new FileReader();
			reader.onload = (e) => {
				const result = e.target?.result as string;
				if (result) {
					this.loadImage(result).then(resolve).catch(reject);
				} else {
					reject(new Error('Failed to read file'));
				}
			};
			reader.onerror = reject;
			reader.readAsDataURL(file);
		});
	}

	private saveState(): void {
		// Remove any states after current index (for redo support)
		if (this.historyIndex < this.history.length - 1) {
			this.history = this.history.slice(0, this.historyIndex + 1);
		}

		// Add new state
		this.history.push(this.ctx.getImageData(0, 0, this.canvas.width, this.canvas.height));

		// Limit history size
		if (this.history.length > this.maxHistorySize) {
			this.history.shift();
		} else {
			this.historyIndex++;
		}
	}

	undo(): boolean {
		if (this.historyIndex > 0) {
			this.historyIndex--;
			this.ctx.putImageData(this.history[this.historyIndex], 0, 0);
			return true;
		}
		return false;
	}

	redo(): boolean {
		if (this.historyIndex < this.history.length - 1) {
			this.historyIndex++;
			this.ctx.putImageData(this.history[this.historyIndex], 0, 0);
			return true;
		}
		return false;
	}

	canUndo(): boolean {
		return this.historyIndex > 0;
	}

	canRedo(): boolean {
		return this.historyIndex < this.history.length - 1;
	}

	rotateClockwise(): void {
		this.rotate(90);
	}

	rotateCounterClockwise(): void {
		this.rotate(-90);
	}

	private rotate(degrees: 90 | -90): void {
		const tempCanvas = document.createElement('canvas');
		const tempCtx = tempCanvas.getContext('2d')!;

		tempCanvas.width = this.canvas.height;
		tempCanvas.height = this.canvas.width;

		tempCtx.translate(tempCanvas.width / 2, tempCanvas.height / 2);
		tempCtx.rotate((degrees * Math.PI) / 180);
		tempCtx.drawImage(this.canvas, -this.canvas.width / 2, -this.canvas.height / 2);

		this.canvas.width = tempCanvas.width;
		this.canvas.height = tempCanvas.height;
		this.ctx.drawImage(tempCanvas, 0, 0);

		this.saveState();
	}

	flipHorizontal(): void {
		this.flip('horizontal');
	}

	flipVertical(): void {
		this.flip('vertical');
	}

	private flip(direction: 'horizontal' | 'vertical'): void {
		const tempCanvas = document.createElement('canvas');
		const tempCtx = tempCanvas.getContext('2d')!;

		tempCanvas.width = this.canvas.width;
		tempCanvas.height = this.canvas.height;

		tempCtx.translate(
			direction === 'horizontal' ? tempCanvas.width : 0,
			direction === 'vertical' ? tempCanvas.height : 0
		);
		tempCtx.scale(direction === 'horizontal' ? -1 : 1, direction === 'vertical' ? -1 : 1);
		tempCtx.drawImage(this.canvas, 0, 0);

		this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
		this.ctx.drawImage(tempCanvas, 0, 0);

		this.saveState();
	}

	resize(width: number, height: number): void {
		const tempCanvas = document.createElement('canvas');
		const tempCtx = tempCanvas.getContext('2d')!;

		tempCanvas.width = width;
		tempCanvas.height = height;

		// Use better quality downsampling
		tempCtx.imageSmoothingEnabled = true;
		tempCtx.imageSmoothingQuality = 'high';

		tempCtx.drawImage(this.canvas, 0, 0, width, height);

		this.canvas.width = width;
		this.canvas.height = height;
		this.ctx.drawImage(tempCanvas, 0, 0);

		this.saveState();
	}

	crop(selection: CropSelection): void {
		const tempCanvas = document.createElement('canvas');
		const tempCtx = tempCanvas.getContext('2d')!;

		tempCanvas.width = selection.width;
		tempCanvas.height = selection.height;

		tempCtx.drawImage(
			this.canvas,
			selection.x,
			selection.y,
			selection.width,
			selection.height,
			0,
			0,
			selection.width,
			selection.height
		);

		this.canvas.width = selection.width;
		this.canvas.height = selection.height;
		this.ctx.drawImage(tempCanvas, 0, 0);

		this.saveState();
	}

	getDimensions(): ImageDimensions {
		return {
			width: this.canvas.width,
			height: this.canvas.height
		};
	}

	toBlob(type = 'image/png', quality = 0.92): Promise<Blob> {
		return new Promise((resolve, reject) => {
			this.canvas.toBlob(
				(blob) => {
					if (blob) {
						resolve(blob);
					} else {
						reject(new Error('Failed to create blob from canvas'));
					}
				},
				type,
				quality
			);
		});
	}

	toFile(filename: string, type = 'image/png'): Promise<File> {
		return this.toBlob(type).then((blob) => new File([blob], filename, { type }));
	}

	reset(): void {
		if (this.originalImage) {
			this.canvas.width = this.originalImage.width;
			this.canvas.height = this.originalImage.height;
			this.ctx.drawImage(this.originalImage, 0, 0);
			this.history = [];
			this.historyIndex = -1;
			this.saveState();
		}
	}
}
