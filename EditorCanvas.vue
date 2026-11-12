<script setup lang="ts">
import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const canvasRef = ref<HTMLCanvasElement | null>(null);
const stickerPreviewRef = ref<HTMLCanvasElement | null>(null);

interface EditState {
  crop: { x: number; y: number; width: number; height: number } | null;
  exposure: number;
  temperature: number;
  brightness: number;
  contrast: number;
  saturation: number;
  vibrance: number;
  rotation_degrees: number;
  strokes: unknown[];
  stickers: Sticker[];
}

interface Sticker {
  x: number;
  y: number;
  width: number;
  height: number;
  color: [number, number, number, number];
  image: number[];
  image_width: number;
  image_height: number;
}

const selectedSticker = ref<Sticker | null>(null);

const currentState = ref<EditState>({
  crop: null,
  exposure: 0.0,
  temperature: 6500.0,
  brightness: 0.0,
  contrast: 1.0,
  saturation: 1.0,
  vibrance: 0.0,
  rotation_degrees: 0.0,
  strokes: [],
  stickers: []
});

let canvasWidth = 0;
let canvasHeight = 0;

async function openImage(path: string) {
  const [w, h] = await invoke<[number, number]>('init_editor', { path });
  canvasWidth = w;
  canvasHeight = h;
  if (canvasRef.value) {
    canvasRef.value.width = w;
    canvasRef.value.height = h;
  }
  await updatePreview();
}

async function updatePreview() {
  if (!canvasRef.value) return;

  // Receives raw binary RGBA array directly
  const rawBytes = await invoke<number[]>('render_live_preview', {
    editState: currentState.value
  });

  const ctx = canvasRef.value.getContext('2d');
  if (ctx) {
    const clamped = new Uint8ClampedArray(rawBytes);
    const imgData = new ImageData(clamped, canvasWidth, canvasHeight);
    ctx.putImageData(imgData, 0, 0);
  }
}

async function extractSticker(event: MouseEvent) {
  const canvas = canvasRef.value;
  if (!canvas) return;
  const bounds = canvas.getBoundingClientRect();
  const seedU = (event.clientX - bounds.left) / bounds.width;
  const seedV = (event.clientY - bounds.top) / bounds.height;
  try {
    const extracted = await invoke<Sticker>('extract_sticker', { seedU, seedV });
    const sticker = { ...extracted, color: [0, 0, 0, 0] as [number, number, number, number] };
    currentState.value.stickers.push(sticker);
    selectedSticker.value = sticker;
    await updatePreview();
  } catch (error) {
    console.error('Unable to extract sticker:', error);
  }
}

function stickerBytes(sticker: Sticker): number[] {
  if (!sticker.image || !sticker.image_width || !sticker.image_height) {
    throw new Error('The selected sticker has no image data.');
  }
  return sticker.image;
}

function stickerDimensions(sticker: Sticker): { width: number; height: number } {
  if (!sticker.image_width || !sticker.image_height) {
    throw new Error('The selected sticker has invalid dimensions.');
  }
  return { width: sticker.image_width, height: sticker.image_height };
}

async function saveSticker() {
  const sticker = selectedSticker.value;
  if (!sticker) return;
  try {
    const { width, height } = stickerDimensions(sticker);
    await invoke('save_sticker', {
      image: stickerBytes(sticker),
      width,
      height
    });
  } catch (error) {
    console.error('Unable to save sticker:', error);
  }
}

async function copyStickerToClipboard() {
  const sticker = selectedSticker.value;
  if (!sticker) return;
  try {
    const { width, height } = stickerDimensions(sticker);
    await invoke('copy_sticker_to_clipboard', {
      image: stickerBytes(sticker),
      width,
      height
    });
  } catch (error) {
    console.error('Unable to copy sticker:', error);
  }
}

watch(selectedSticker, (sticker) => {
  const canvas = stickerPreviewRef.value;
  if (!canvas || !sticker) return;
  canvas.width = sticker.image_width;
  canvas.height = sticker.image_height;
  canvas.getContext('2d')?.putImageData(
    new ImageData(new Uint8ClampedArray(stickerBytes(sticker)), sticker.image_width, sticker.image_height),
    0,
    0
  );
}, { flush: 'post' });
</script>

<template>
  <div class="editor-container">
    <canvas ref="canvasRef" @click="extractSticker"></canvas>
    <p class="hint">Click an image region to extract it as a sticker.</p>
    <div class="controls">
      <label>Exposure: {{ currentState.exposure }}</label>
      <input 
        type="range" 
        min="-3" 
        max="3" 
        step="0.1" 
        v-model.number="currentState.exposure" 
        @input="updatePreview" 
      />

      <label>Rotation: {{ currentState.rotation_degrees }}°</label>
      <input 
        type="range" 
        min="-180" 
        max="180" 
        step="1" 
        v-model.number="currentState.rotation_degrees"
        @input="updatePreview" 
      />

      <div class="sticker-preview">
        <p>Sticker preview</p>
        <canvas v-if="selectedSticker" ref="stickerPreviewRef"></canvas>
        <span v-else>Click the image to extract a sticker.</span>
        <div class="sticker-actions">
          <button :disabled="!selectedSticker" @click="saveSticker">Save to ./stickers</button>
          <button :disabled="!selectedSticker" @click="copyStickerToClipboard">Copy to clipboard</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sticker-preview {
  margin-top: 1rem;
  padding: 0.75rem;
  border: 1px solid #ccc;
  min-height: 120px;
  text-align: center;
}

.sticker-preview canvas {
  max-width: 100%;
  max-height: 180px;
  background: repeating-conic-gradient(#eee 0% 25%, #fff 0% 50%) 50% / 16px 16px;
  image-rendering: pixelated;
}

.sticker-actions {
  display: flex;
  gap: 0.5rem;
  justify-content: center;
  margin-top: 0.75rem;
}
</style>