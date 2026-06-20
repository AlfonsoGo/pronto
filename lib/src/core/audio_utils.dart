import 'dart:typed_data';

/// Convierte bytes PCM 16-bit little-endian (mono) a float32 normalizado
/// a [-1.0, 1.0], que es lo que espera whisper.cpp.
Float32List pcm16ToFloat32(Uint8List pcmBytes) {
  final sampleCount = pcmBytes.lengthInBytes ~/ 2;
  final out = Float32List(sampleCount);
  final view = ByteData.sublistView(pcmBytes);
  for (var i = 0; i < sampleCount; i++) {
    final s = view.getInt16(i * 2, Endian.little);
    out[i] = s / 32768.0;
  }
  return out;
}

/// Concatena varios chunks de bytes PCM en uno solo.
Uint8List concatChunks(List<Uint8List> chunks) {
  var total = 0;
  for (final c in chunks) {
    total += c.lengthInBytes;
  }
  final out = Uint8List(total);
  var offset = 0;
  for (final c in chunks) {
    out.setRange(offset, offset + c.lengthInBytes, c);
    offset += c.lengthInBytes;
  }
  return out;
}
