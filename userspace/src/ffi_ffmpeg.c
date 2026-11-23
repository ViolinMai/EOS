#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libswscale/swscale.h>
#include <libavutil/imgutils.h>
#include <libavutil/log.h>
#include <libavutil/pixdesc.h>

#define AUDIO_RING_SAMPLES 131072

typedef struct {
    AVFormatContext *fmt_ctx;
    AVCodecContext *dec_ctx;
    AVCodecContext *aud_ctx;
    struct SwsContext *sws_ctx;
    AVFrame *frame;
    AVFrame *aud_frame;
    AVPacket *pkt;
    int video_stream_idx;
    int audio_stream_idx;
    int src_w;
    int src_h;
    int last_dst_w;
    int last_dst_h;
    double fps;
    int64_t duration_ms;
    int64_t video_bitrate;
    int64_t audio_bitrate;
    int audio_sample_rate;
    int audio_channels;
    int bit_depth;
    char codec_name[64];
    char audio_codec_name[64];
    char pix_fmt_name[64];
    char color_space_name[64];
    char color_range_name[32];
    int16_t audio_ring[AUDIO_RING_SAMPLES];
    int aud_head;
    int aud_tail;
} EosVideoCtx;

static void push_audio_sample(EosVideoCtx *ctx, int16_t s) {
    int next = (ctx->aud_head + 1) % AUDIO_RING_SAMPLES;
    if (next != ctx->aud_tail) {
        ctx->audio_ring[ctx->aud_head] = s;
        ctx->aud_head = next;
    }
}

static int16_t f32_to_i16(float f) {
    if (f > 1.0f) f = 1.0f;
    if (f < -1.0f) f = -1.0f;
    return (int16_t)(f * 32767.0f);
}

static void decode_audio_packet(EosVideoCtx *ctx, AVPacket *pkt) {
    if (!ctx->aud_ctx || !ctx->aud_frame) return;
    if (avcodec_send_packet(ctx->aud_ctx, pkt) < 0) return;

    while (avcodec_receive_frame(ctx->aud_ctx, ctx->aud_frame) == 0) {
        AVFrame *af = ctx->aud_frame;
        int n = af->nb_samples;
        int sr = af->sample_rate > 0 ? af->sample_rate : 48000;
        int ch = af->ch_layout.nb_channels > 0 ? af->ch_layout.nb_channels : 2;
        int fmt = af->format;

        // Resample to 48000 Hz stereo S16 on the fly (Zero external libswresample dependency)
        int out_n = (int)(((int64_t)n * 48000) / sr);
        for (int i = 0; i < out_n; i++) {
            int si = (int)(((int64_t)i * sr) / 48000);
            if (si >= n) si = n - 1;
            int16_t l = 0, r = 0;
            if (fmt == AV_SAMPLE_FMT_FLTP) {
                float *p0 = (float *)af->data[0];
                float *p1 = (ch > 1 && af->data[1]) ? (float *)af->data[1] : p0;
                if (p0) l = f32_to_i16(p0[si]);
                if (p1) r = f32_to_i16(p1[si]);
            } else if (fmt == AV_SAMPLE_FMT_FLT && af->data[0]) {
                float *p = (float *)af->data[0];
                l = f32_to_i16(p[si * ch]);
                r = f32_to_i16(p[si * ch + (ch > 1 ? 1 : 0)]);
            } else if (fmt == AV_SAMPLE_FMT_S16 && af->data[0]) {
                int16_t *p = (int16_t *)af->data[0];
                l = p[si * ch];
                r = p[si * ch + (ch > 1 ? 1 : 0)];
            } else if (fmt == AV_SAMPLE_FMT_S16P) {
                int16_t *p0 = (int16_t *)af->data[0];
                int16_t *p1 = (ch > 1 && af->data[1]) ? (int16_t *)af->data[1] : p0;
                if (p0) l = p0[si];
                if (p1) r = p1[si];
            }
            push_audio_sample(ctx, l);
            push_audio_sample(ctx, r);
        }
        av_frame_unref(af);
    }
}

EosVideoCtx* eos_video_open(const char *path) {
    if (!path) return NULL;
    av_log_set_level(AV_LOG_QUIET);

    EosVideoCtx *ctx = (EosVideoCtx *)calloc(1, sizeof(EosVideoCtx));
    if (!ctx) return NULL;
    ctx->video_stream_idx = -1;
    ctx->audio_stream_idx = -1;

    if (avformat_open_input(&ctx->fmt_ctx, path, NULL, NULL) < 0) {
        free(ctx);
        return NULL;
    }

    if (avformat_find_stream_info(ctx->fmt_ctx, NULL) < 0) {
        avformat_close_input(&ctx->fmt_ctx);
        free(ctx);
        return NULL;
    }

    for (unsigned int i = 0; i < ctx->fmt_ctx->nb_streams; i++) {
        enum AVMediaType mt = ctx->fmt_ctx->streams[i]->codecpar->codec_type;
        if (mt == AVMEDIA_TYPE_VIDEO && ctx->video_stream_idx < 0) {
            ctx->video_stream_idx = (int)i;
        } else if (mt == AVMEDIA_TYPE_AUDIO && ctx->audio_stream_idx < 0) {
            ctx->audio_stream_idx = (int)i;
        }
    }

    if (ctx->video_stream_idx < 0) {
        avformat_close_input(&ctx->fmt_ctx);
        free(ctx);
        return NULL;
    }

    AVStream *st = ctx->fmt_ctx->streams[ctx->video_stream_idx];
    const AVCodec *codec = avcodec_find_decoder(st->codecpar->codec_id);
    if (!codec) {
        avformat_close_input(&ctx->fmt_ctx);
        free(ctx);
        return NULL;
    }

    ctx->dec_ctx = avcodec_alloc_context3(codec);
    if (!ctx->dec_ctx || avcodec_parameters_to_context(ctx->dec_ctx, st->codecpar) < 0) {
        if (ctx->dec_ctx) avcodec_free_context(&ctx->dec_ctx);
        avformat_close_input(&ctx->fmt_ctx);
        free(ctx);
        return NULL;
    }

    ctx->dec_ctx->thread_count = 1;
    ctx->dec_ctx->thread_type = 0;
    ctx->dec_ctx->flags2 |= AV_CODEC_FLAG2_FAST;
    if (avcodec_open2(ctx->dec_ctx, codec, NULL) < 0) {
        avcodec_free_context(&ctx->dec_ctx);
        avformat_close_input(&ctx->fmt_ctx);
        free(ctx);
        return NULL;
    }

    if (ctx->audio_stream_idx >= 0) {
        AVStream *ast = ctx->fmt_ctx->streams[ctx->audio_stream_idx];
        const AVCodec *ac = avcodec_find_decoder(ast->codecpar->codec_id);
        if (ac) {
            ctx->aud_ctx = avcodec_alloc_context3(ac);
            if (ctx->aud_ctx && avcodec_parameters_to_context(ctx->aud_ctx, ast->codecpar) >= 0) {
                ctx->aud_ctx->thread_count = 1;
                ctx->aud_ctx->thread_type = 0;
                if (avcodec_open2(ctx->aud_ctx, ac, NULL) < 0) {
                    avcodec_free_context(&ctx->aud_ctx);
                }
            }
        }
    }

    ctx->frame = av_frame_alloc();
    ctx->aud_frame = av_frame_alloc();
    ctx->pkt = av_packet_alloc();
    ctx->src_w = ctx->dec_ctx->width;
    ctx->src_h = ctx->dec_ctx->height;

    if (st->avg_frame_rate.den > 0 && st->avg_frame_rate.num > 0) {
        ctx->fps = av_q2d(st->avg_frame_rate);
    } else if (st->r_frame_rate.den > 0 && st->r_frame_rate.num > 0) {
        ctx->fps = av_q2d(st->r_frame_rate);
    } else {
        ctx->fps = 30.0;
    }

    if (ctx->fmt_ctx->duration > 0) {
        ctx->duration_ms = ctx->fmt_ctx->duration / 1000;
    } else if (st->duration > 0) {
        ctx->duration_ms = (int64_t)(st->duration * av_q2d(st->time_base) * 1000.0);
    } else {
        ctx->duration_ms = 0;
    }

    ctx->video_bitrate = st->codecpar->bit_rate > 0 ? st->codecpar->bit_rate : ctx->fmt_ctx->bit_rate;
    enum AVPixelFormat pfmt = (enum AVPixelFormat)st->codecpar->format;
    const char *pfmt_str = av_get_pix_fmt_name(pfmt);
    strncpy(ctx->pix_fmt_name, pfmt_str ? pfmt_str : "unknown", sizeof(ctx->pix_fmt_name) - 1);

    const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(pfmt);
    ctx->bit_depth = (desc && desc->comp[0].depth > 0) ? desc->comp[0].depth : 8;

    const char *cs_str = av_color_space_name(st->codecpar->color_space);
    strncpy(ctx->color_space_name, cs_str ? cs_str : "unknown", sizeof(ctx->color_space_name) - 1);

    const char *cr_str = av_color_range_name(st->codecpar->color_range);
    strncpy(ctx->color_range_name, cr_str ? cr_str : "unknown", sizeof(ctx->color_range_name) - 1);

    if (ctx->audio_stream_idx >= 0) {
        AVStream *ast = ctx->fmt_ctx->streams[ctx->audio_stream_idx];
        const AVCodec *ac = avcodec_find_decoder(ast->codecpar->codec_id);
        strncpy(ctx->audio_codec_name, (ac && ac->name) ? ac->name : "none", sizeof(ctx->audio_codec_name) - 1);
        ctx->audio_sample_rate = ast->codecpar->sample_rate;
        ctx->audio_channels = ast->codecpar->ch_layout.nb_channels;
        ctx->audio_bitrate = ast->codecpar->bit_rate;
    } else {
        strcpy(ctx->audio_codec_name, "none");
    }

    const char *cname = codec->name ? codec->name : "unknown";
    strncpy(ctx->codec_name, cname, sizeof(ctx->codec_name) - 1);
    ctx->codec_name[sizeof(ctx->codec_name) - 1] = '\0';

    return ctx;
}

void eos_video_get_info(EosVideoCtx *ctx, int *w, int *h, double *fps, int64_t *dur_ms, char *codec_buf, int buf_len) {
    if (!ctx) return;
    if (w) *w = ctx->src_w;
    if (h) *h = ctx->src_h;
    if (fps) *fps = ctx->fps;
    if (dur_ms) *dur_ms = ctx->duration_ms;
    if (codec_buf && buf_len > 0) {
        strncpy(codec_buf, ctx->codec_name, (size_t)(buf_len - 1));
        codec_buf[buf_len - 1] = '\0';
    }
}

void eos_video_get_extended_info(
    EosVideoCtx *ctx,
    int64_t *v_bitrate,
    int *bit_depth,
    int *a_sr,
    int *a_ch,
    int64_t *a_bitrate,
    char *pix_fmt_buf,
    char *cs_buf,
    char *cr_buf,
    char *acodec_buf,
    int buf_len
) {
    if (!ctx) return;
    if (v_bitrate) *v_bitrate = ctx->video_bitrate;
    if (bit_depth) *bit_depth = ctx->bit_depth;
    if (a_sr) *a_sr = ctx->audio_sample_rate;
    if (a_ch) *a_ch = ctx->audio_channels;
    if (a_bitrate) *a_bitrate = ctx->audio_bitrate;
    if (pix_fmt_buf && buf_len > 0) {
        strncpy(pix_fmt_buf, ctx->pix_fmt_name, (size_t)(buf_len - 1));
        pix_fmt_buf[buf_len - 1] = '\0';
    }
    if (cs_buf && buf_len > 0) {
        strncpy(cs_buf, ctx->color_space_name, (size_t)(buf_len - 1));
        cs_buf[buf_len - 1] = '\0';
    }
    if (cr_buf && buf_len > 0) {
        strncpy(cr_buf, ctx->color_range_name, (size_t)(buf_len - 1));
        cr_buf[buf_len - 1] = '\0';
    }
    if (acodec_buf && buf_len > 0) {
        strncpy(acodec_buf, ctx->audio_codec_name, (size_t)(buf_len - 1));
        acodec_buf[buf_len - 1] = '\0';
    }
}

int eos_video_seek_ms(EosVideoCtx *ctx, int64_t target_ms) {
    if (!ctx || !ctx->fmt_ctx) return -1;
    if (target_ms < 0) target_ms = 0;
    if (av_seek_frame(ctx->fmt_ctx, -1, target_ms * 1000, AVSEEK_FLAG_BACKWARD) < 0) {
        return -1;
    }
    avcodec_flush_buffers(ctx->dec_ctx);
    if (ctx->aud_ctx) avcodec_flush_buffers(ctx->aud_ctx);
    ctx->aud_head = 0;
    ctx->aud_tail = 0;
    return 0;
}

int eos_video_pop_audio(EosVideoCtx *ctx, int16_t *out_pcm, int max_samples) {
    if (!ctx || !out_pcm || max_samples <= 0) return 0;
    int count = 0;
    while (count < max_samples && ctx->aud_tail != ctx->aud_head) {
        out_pcm[count++] = ctx->audio_ring[ctx->aud_tail];
        ctx->aud_tail = (ctx->aud_tail + 1) % AUDIO_RING_SAMPLES;
    }
    return count;
}

int eos_video_next_frame(EosVideoCtx *ctx, uint32_t *out_argb, int dst_w, int dst_h, int64_t *out_pts_ms) {
    if (!ctx || !out_argb || dst_w <= 0 || dst_h <= 0) return -1;

    while (1) {
        int ret = avcodec_receive_frame(ctx->dec_ctx, ctx->frame);
        if (ret == 0) {
            if (ctx->frame->width <= 0 || ctx->frame->height <= 0 || ctx->frame->format < 0 || !ctx->frame->data[0]) {
                av_frame_unref(ctx->frame);
                continue;
            }
            if (!ctx->sws_ctx || ctx->last_dst_w != dst_w || ctx->last_dst_h != dst_h) {
                if (ctx->sws_ctx) sws_freeContext(ctx->sws_ctx);
                ctx->sws_ctx = sws_getContext(
                    ctx->frame->width, ctx->frame->height, (enum AVPixelFormat)ctx->frame->format,
                    dst_w, dst_h, AV_PIX_FMT_BGRA,
                    SWS_FAST_BILINEAR, NULL, NULL, NULL
                );
                ctx->last_dst_w = dst_w;
                ctx->last_dst_h = dst_h;
            }
            if (!ctx->sws_ctx) return -1;

            uint8_t *dst_data[4] = { (uint8_t *)out_argb, NULL, NULL, NULL };
            int dst_linesize[4] = { dst_w * 4, 0, 0, 0 };
            sws_scale(
                ctx->sws_ctx,
                (const uint8_t *const *)ctx->frame->data,
                ctx->frame->linesize,
                0,
                ctx->frame->height,
                dst_data,
                dst_linesize
            );

            if (out_pts_ms) {
                AVStream *st = ctx->fmt_ctx->streams[ctx->video_stream_idx];
                int64_t pts = ctx->frame->best_effort_timestamp;
                if (pts != AV_NOPTS_VALUE) {
                    *out_pts_ms = (int64_t)(pts * av_q2d(st->time_base) * 1000.0);
                }
            }
            av_frame_unref(ctx->frame);
            return 1;
        } else if (ret == AVERROR_EOF) {
            return 0;
        } else if (ret != AVERROR(EAGAIN)) {
            return -1;
        }

        int read_ret = av_read_frame(ctx->fmt_ctx, ctx->pkt);
        if (read_ret < 0) {
            avcodec_send_packet(ctx->dec_ctx, NULL);
            continue;
        }

        if (ctx->pkt->stream_index == ctx->video_stream_idx) {
            avcodec_send_packet(ctx->dec_ctx, ctx->pkt);
        } else if (ctx->pkt->stream_index == ctx->audio_stream_idx) {
            decode_audio_packet(ctx, ctx->pkt);
        }
        av_packet_unref(ctx->pkt);
    }
}

void eos_video_close(EosVideoCtx *ctx) {
    if (!ctx) return;
    if (ctx->sws_ctx) sws_freeContext(ctx->sws_ctx);
    if (ctx->frame) av_frame_free(&ctx->frame);
    if (ctx->aud_frame) av_frame_free(&ctx->aud_frame);
    if (ctx->pkt) av_packet_free(&ctx->pkt);
    if (ctx->dec_ctx) avcodec_free_context(&ctx->dec_ctx);
    if (ctx->aud_ctx) avcodec_free_context(&ctx->aud_ctx);
    if (ctx->fmt_ctx) avformat_close_input(&ctx->fmt_ctx);
    free(ctx);
}
