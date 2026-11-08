/* Compile-only ABI proof against the selected libmpv0.41 headers; no native role. */
#include <stddef.h>
#include <mpv/client.h>
#include <mpv/render.h>
#include <mpv/render_gl.h>
_Static_assert(MPV_CLIENT_API_VERSION == MPV_MAKE_VERSION(2,5), "selected client ABI");
_Static_assert(sizeof(mpv_render_param)==16 && offsetof(mpv_render_param,data)==8,"render parameter ABI");
_Static_assert(sizeof(mpv_opengl_init_params)==16,"OpenGL init ABI");
_Static_assert(sizeof(mpv_opengl_fbo)==16,"framebuffer ABI");
_Static_assert(sizeof(mpv_event)==24 && offsetof(mpv_event,data)==16,"event ABI");
_Static_assert(sizeof(mpv_event_property)==24 && offsetof(mpv_event_property,data)==16,"property ABI");
_Static_assert(sizeof(mpv_render_frame_info)==16,"frame timing ABI");
_Static_assert(MPV_FORMAT_FLAG==3 && MPV_EVENT_PROPERTY_CHANGE==22 && MPV_EVENT_QUEUE_OVERFLOW==24,"event/format constants");
_Static_assert(MPV_RENDER_PARAM_API_TYPE==1 && MPV_RENDER_PARAM_OPENGL_INIT_PARAMS==2 && MPV_RENDER_PARAM_OPENGL_FBO==3 && MPV_RENDER_PARAM_FLIP_Y==4,"render ABI constants");
_Static_assert(MPV_RENDER_PARAM_WL_DISPLAY==9 && MPV_RENDER_PARAM_NEXT_FRAME_INFO==11 && MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME==12 && MPV_RENDER_PARAM_SKIP_RENDERING==13,"render lifecycle constants");
_Static_assert(_Generic(&mpv_get_time_us, int64_t (*)(mpv_handle*):1,default:0),"time query signature");
_Static_assert(_Generic(&mpv_set_property_async, int (*)(mpv_handle*,uint64_t,const char*,mpv_format,void*):1,default:0),"async pause signature");
_Static_assert(_Generic(&mpv_command_async,int (*)(mpv_handle*,uint64_t,const char**):1,default:0),"async load signature");
