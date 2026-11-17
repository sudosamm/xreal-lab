#include "my_application.h"

int main(int argc, char** argv) {
  // xreal-lab (16/11/2026): no Flutter 3.50, com placa NVIDIA, o embedder Linux não usa
  // glBlitFramebuffer e cai no caminho do shader (fl_compositor_opengl.cc, composite_layer),
  // cuja conta de deslocamento põe o centro da imagem no canto inferior esquerdo da janela
  // (visto em X11 e em Wayland). O renderizador por software do próprio Flutter evita isso.
  // Para voltar ao OpenGL: FLUTTER_LINUX_RENDERER=opengl.
  g_setenv("FLUTTER_LINUX_RENDERER", "software", FALSE);

  g_autoptr(MyApplication) app = my_application_new();
  return g_application_run(G_APPLICATION(app), argc, argv);
}
