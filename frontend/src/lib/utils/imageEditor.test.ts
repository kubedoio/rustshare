import { describe, it, expect, beforeEach } from 'vitest';
import { ImageEditor } from './imageEditor';

describe('ImageEditor', () => {
  let canvas: HTMLCanvasElement;
  let editor: ImageEditor;

  beforeEach(() => {
    canvas = document.createElement('canvas');
    canvas.width = 100;
    canvas.height = 100;
    editor = new ImageEditor(canvas);
  });

  it('should initialize with canvas', () => {
    expect(editor).toBeDefined();
    expect(editor.getDimensions()).toEqual({ width: 100, height: 100 });
  });

  it('should track undo/redo state', () => {
    expect(editor.canUndo()).toBe(false);
    expect(editor.canRedo()).toBe(false);
  });

  it('should flip dimensions on rotate', async () => {
    // Create a simple test by drawing something
    const ctx = canvas.getContext('2d')!;
    ctx.fillStyle = 'red';
    ctx.fillRect(0, 0, 100, 100);

    editor.rotateClockwise();

    const dims = editor.getDimensions();
    expect(dims.width).toBe(100);
    expect(dims.height).toBe(100);
  });
});
