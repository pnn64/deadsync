#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float PI=3.14159265358979323846f, TINY_PERCENT_BASE=0.5f, TINY_PERCENT_GATE=1;
constexpr float BEAT_OFFSET_HEIGHT=15, BEAT_Y_OFFSET_HEIGHT=15, BEAT_Z_OFFSET_HEIGHT=15;
constexpr float BEAT_PI_HEIGHT=2, BEAT_Y_PI_HEIGHT=2, BEAT_Z_PI_HEIGHT=2;
constexpr int dim_x=0, dim_y=1, dim_z=2;
struct PerPlayerData { float m_fBeatFactor[3]={}; };
struct SongPosition { float m_fSongBeatVisible; };
struct PlayerOptions { enum { EFFECT_BEAT, EFFECT_BEAT_Y, EFFECT_BEAT_Z,
    EFFECT_BEAT_PERIOD, EFFECT_BEAT_Y_PERIOD, EFFECT_BEAT_Z_PERIOD, EFFECT_TINY }; };
struct Column { float fXOffset; };
struct PlayerState { float m_NotefieldZoom=1; };
struct Position { float x,y,z; };
#define SCALE(x, l1, h1, l2, h2) \
  (((x) - (l1)) * ((h2) - (l2)) / ((h1) - (l1)) + (l2))
static void UpdateBeat(
    int dimension, PerPlayerData& data, const SongPosition& position,
    float beat_offset, float beat_mult) {
  float fAccelTime = 0.2f, fTotalTime = 0.5f;
  float fBeat =
      ((position.m_fSongBeatVisible + fAccelTime + beat_offset) *
       (beat_mult + 1));

  const bool bEvenBeat = (int(fBeat) % 2) != 0;

  data.m_fBeatFactor[dimension] = 0;
  if (fBeat < 0) {
    return;
  }

  // -100.2 -> -0.2 -> 0.2
  fBeat -= std::trunc(fBeat);
  fBeat += 1;
  fBeat -= std::trunc(fBeat);

  if (fBeat >= fTotalTime) {
    return;
  }

  if (fBeat < fAccelTime) {
    data.m_fBeatFactor[dimension] = SCALE(fBeat, 0.0f, fAccelTime, 0.0f, 1.0f);
    data.m_fBeatFactor[dimension] *= data.m_fBeatFactor[dimension];
  } else /* fBeat < fTotalTime */ {
    data.m_fBeatFactor[dimension] =
        SCALE(fBeat, fAccelTime, fTotalTime, 1.0f, 0.0f);
    data.m_fBeatFactor[dimension] = 1 - (1 - data.m_fBeatFactor[dimension]) *
                                            (1 - data.m_fBeatFactor[dimension]);
  }

  if (bEvenBeat) {
    data.m_fBeatFactor[dimension] *= -1;
  }
  data.m_fBeatFactor[dimension] *= 20.0f;
}
Position PositionFor(const PerPlayerData& data,const float* fEffects,float travel,float col_x,float direction) {
    const Column columns[]={{col_x}};
    const Column* pCols=columns;
    const int iColNum=0;
    const PlayerState player;
    const PlayerState* pPlayerState=&player;
    const float fYOffset=travel;
    float fPixelOffsetFromCenter=0, f=direction*travel+12, fZPos=0;
  if (fEffects[PlayerOptions::EFFECT_BEAT] != 0) {
    const float fShift =
        data.m_fBeatFactor[dim_x] *
        std::sin(
            fYOffset / ((fEffects[PlayerOptions::EFFECT_BEAT_PERIOD] *
                         BEAT_OFFSET_HEIGHT) +
                        BEAT_OFFSET_HEIGHT) +
            PI / BEAT_PI_HEIGHT);
    fPixelOffsetFromCenter += fEffects[PlayerOptions::EFFECT_BEAT] * fShift;
  }
  fPixelOffsetFromCenter +=
      pCols[iColNum].fXOffset * pPlayerState->m_NotefieldZoom;

  if (fEffects[PlayerOptions::EFFECT_TINY] != 0) {
    // Allow Tiny to pull tracks together, but not to push them apart.
    float fTinyPercent = fEffects[PlayerOptions::EFFECT_TINY];
    fTinyPercent = std::min(
        std::pow(TINY_PERCENT_BASE, fTinyPercent), (float)TINY_PERCENT_GATE);
    fPixelOffsetFromCenter *= fTinyPercent;
  }

  if (fEffects[PlayerOptions::EFFECT_BEAT_Y] != 0) {
    const float fShift =
        data.m_fBeatFactor[dim_y] *
        std::sin(
            fYOffset / ((fEffects[PlayerOptions::EFFECT_BEAT_Y_PERIOD] *
                         BEAT_Y_OFFSET_HEIGHT) +
                        BEAT_Y_OFFSET_HEIGHT) +
            PI / BEAT_Y_PI_HEIGHT);
    f += fEffects[PlayerOptions::EFFECT_BEAT_Y] * fShift;
  }
  if (fEffects[PlayerOptions::EFFECT_BEAT_Z] != 0) {
    const float fShift =
        data.m_fBeatFactor[dim_z] *
        std::sin(
            fYOffset / ((fEffects[PlayerOptions::EFFECT_BEAT_Z_PERIOD] *
                         BEAT_Z_OFFSET_HEIGHT) +
                        BEAT_Z_OFFSET_HEIGHT) +
            PI / BEAT_Z_PI_HEIGHT);
    fZPos += fEffects[PlayerOptions::EFFECT_BEAT_Z] * fShift;
  }
    return {fPixelOffsetFromCenter,f,fZPos};
}
int main() {
    const float configs[][12]={
      {0,0,0, 0,0,0, 0,0,0, 0,0,0},
      {.75f,-.5f,1.25f, .1f,-.15f,1.3f, .5f,-.25f,1.5f, .5f,-.75f,1.25f},
      {-.25f,.5f,-.75f, -.1f,.15f,-1.3f, -.5f,.25f,-1.5f, -.5f,-.25f,.25f},
      {1,1,1, 0,0,0, -1,-1,-1, 0,0,0},
      {1e-8f,-1e-8f,1e-8f, .1f,-.15f,1.3f, .5f,-.25f,1.5f, .5f,-.75f,1.25f}
    };
    for(const auto& a:configs) for(float beat:{-3.f,-.25f,0.f,.05f,.15f,.3f,.8f,1.f,1.15f,1.95f,2.2f,4.5f})
    for(float col:{-32.f,32.f}) for(float travel:{-128.f,0.f,32.f,128.f,384.f})
    for(float direction:{-1.f,1.f}) for(float tiny:{0.f,1.f}) {
        PerPlayerData data;
        const SongPosition position{beat};
        for(int axis=0;axis<3;axis++) UpdateBeat(axis,data,position,a[axis+3],a[axis+6]);
        const float effects[]={a[0],a[1],a[2],a[9],a[10],a[11],tiny};
        const auto p=PositionFor(data,effects,travel,col,direction);
        const auto receptor=PositionFor(data,effects,0,col,direction);
        const auto tail=PositionFor(data,effects,travel+128,col,direction);
        std::printf("{\"amount_x\":%.9g,\"amount_y\":%.9g,\"amount_z\":%.9g,"
          "\"offset_x\":%.9g,\"offset_y\":%.9g,\"offset_z\":%.9g,"
          "\"mult_x\":%.9g,\"mult_y\":%.9g,\"mult_z\":%.9g,"
          "\"period_x\":%.9g,\"period_y\":%.9g,\"period_z\":%.9g,"
          "\"beat\":%.9g,\"travel\":%.9g,\"col_x\":%.9g,\"direction\":%.9g,\"tiny\":%.9g,\"lane_offset\":12,"
          "\"factor_x\":%.9g,\"factor_y\":%.9g,\"factor_z\":%.9g,"
          "\"x\":%.9g,\"y\":%.9g,\"z\":%.9g,"
          "\"receptor_x\":%.9g,\"receptor_y\":%.9g,\"receptor_z\":%.9g,"
          "\"tail_x\":%.9g,\"tail_y\":%.9g,\"tail_z\":%.9g}\n",
          a[0],a[1],a[2],a[3],a[4],a[5],a[6],a[7],a[8],a[9],a[10],a[11],
          beat,travel,col,direction,tiny,data.m_fBeatFactor[0],data.m_fBeatFactor[1],data.m_fBeatFactor[2],
          p.x,p.y,p.z,receptor.x,receptor.y,receptor.z,tail.x,tail.y,tail.z);
    }
}
