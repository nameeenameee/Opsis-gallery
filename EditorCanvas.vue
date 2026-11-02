<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const canvasRef = ref<HTMLCanvasElement | null>(null);

interface EditState {
  crop: { x: number; y: number; width: number; height: number } | null;
  exposure: number;
  temperature: number;
  brightness: number;
  contrast: number;
  saturation: number;
  vibrance: number;
  rotation: number;
  strokes: any[];
}

const currentState = ref<EditState>({
  crop: null,
  exposure: 0.0,
  temperature: 6500.0,
  brightness: 0.0,
  contrast: 1.0,
  saturation: 1.0,
  vibrance: 0.0,
  rotation: 0.0,
  strokes: []
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
  const rawBytes = await invoke<ArrayBuffer>('render_live_preview', {
    editState: currentState.value
  });

  const ctx = canvasRef.value.getContext('2d');
  if (ctx) {
    const clamped = new Uint8ClampedArray(rawBytes);
    const imgData = new ImageData(clamped, canvasWidth, canvasHeight);
    ctx.putImageData(imgData, 0, 0);
  }
}
</script>

<template>
  <div class="editor-container">
    <canvas ref="canvasRef"></canvas>
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

      <label>Rotation: {{ currentState.rotation }}°</label>
      <input 
        type="range" 
        min="-180" 
        max="180" 
        step="1" 
        v-model.number="currentState.rotation" 
        @input="updatePreview" 
      />
    </div>
  </div>
</template>