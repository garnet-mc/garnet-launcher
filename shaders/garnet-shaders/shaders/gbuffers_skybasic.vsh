#version 120
// The sky is drawn procedurally in the deferred pass; this only needs to
// cover the sky pixels so their depth reads as "far".
void main() {
    gl_Position = ftransform();
}
