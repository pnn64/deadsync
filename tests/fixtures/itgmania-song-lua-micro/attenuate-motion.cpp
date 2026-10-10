#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float ARROW_SIZE=64, TINY_PERCENT_BASE=0.5f, TINY_PERCENT_GATE=1;
struct PlayerOptions { enum { EFFECT_ATTENUATE_X, EFFECT_ATTENUATE_Y, EFFECT_ATTENUATE_Z, EFFECT_TINY }; };
struct Column { float fXOffset; };
struct PlayerState { float m_NotefieldZoom=1; };
struct Position { float x,y,z; };
Position PositionFor(float x_amount,float y_amount,float z_amount,float travel,float col_x,float direction,float tiny) {
    const float values[]={x_amount,y_amount,z_amount,tiny};
    const float* fEffects=values;
    const Column columns[]={{col_x}};
    const Column* pCols=columns;
    const int iCol=0, iColNum=0;
    const PlayerState player;
    const PlayerState* pPlayerState=&player;
    const float fYOffset=travel;
    float fPixelOffsetFromCenter=0, f=direction*travel+12, fZPos=0;
  if (fEffects[PlayerOptions::EFFECT_ATTENUATE_X] != 0) {
    const float fXOffset = pCols[iColNum].fXOffset;
    fPixelOffsetFromCenter += fEffects[PlayerOptions::EFFECT_ATTENUATE_X] *
                              (fYOffset / ARROW_SIZE) *
                              (fYOffset / ARROW_SIZE) * (fXOffset / ARROW_SIZE);
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

  if (fEffects[PlayerOptions::EFFECT_ATTENUATE_Y] != 0) {
    const float fXOffset = pCols[iCol].fXOffset;
    f += fEffects[PlayerOptions::EFFECT_ATTENUATE_Y] * (fYOffset / ARROW_SIZE) *
         (fYOffset / ARROW_SIZE) * (fXOffset / ARROW_SIZE);
  }
  if (fEffects[PlayerOptions::EFFECT_ATTENUATE_Z] != 0) {
    const float fXOffset = pCols[iCol].fXOffset;
    fZPos += fEffects[PlayerOptions::EFFECT_ATTENUATE_Z] *
             (fYOffset / ARROW_SIZE) * (fYOffset / ARROW_SIZE) *
             (fXOffset / ARROW_SIZE);
  }
    return {fPixelOffsetFromCenter,f,fZPos};
}
int main() {
    const float amounts[][3]={{0,0,0},{0.75f,0,0},{0,-0.5f,0},{0,0,1.25f},{0.75f,-0.5f,1.25f},{-0.75f,0.5f,-1.25f},{1e-8f,-1e-8f,1e-8f}};
    for(const auto& a:amounts) for(float col:{-224.f,-96.f,-32.f,0.f,32.f,96.f,224.f})
    for(float travel:{-128.f,0.f,32.f,128.f,384.f}) for(float direction:{-1.f,1.f}) for(float tiny:{0.f,1.f}) {
        const auto p=PositionFor(a[0],a[1],a[2],travel,col,direction,tiny);
        std::printf("{\"amount_x\":%.9g,\"amount_y\":%.9g,\"amount_z\":%.9g,\"travel\":%.9g,\"col_x\":%.9g,\"direction\":%.9g,\"tiny\":%.9g,\"lane_offset\":12,\"x\":%.9g,\"y\":%.9g,\"z\":%.9g}\n",a[0],a[1],a[2],travel,col,direction,tiny,p.x,p.y,p.z);
    }
}
