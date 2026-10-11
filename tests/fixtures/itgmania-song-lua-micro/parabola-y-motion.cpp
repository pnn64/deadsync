#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float WAVE_MOD_MAGNITUDE=20, WAVE_MOD_HEIGHT=38, ARROW_SIZE=64;
constexpr float BOOST_MOD_MIN_CLAMP=-400, BOOST_MOD_MAX_CLAMP=400;
constexpr float BRAKE_MOD_MIN_CLAMP=-400, BRAKE_MOD_MAX_CLAMP=400;
constexpr float SCREEN_HEIGHT=480, BOOMERANG_PEAK_PERCENTAGE=0.75f;
float GetNoteFieldHeight() { return 480; }
float SCALE(float x,float l,float h,float a,float b) { return ((x-l)/(h-l))*(b-a)+a; }
void rage_clamp(float& x,float l,float h) { x=std::clamp(x,l,h); }
struct PlayerOptions { enum { ACCEL_BOOST,ACCEL_BRAKE,ACCEL_WAVE,ACCEL_WAVE_PERIOD,ACCEL_BOOMERANG }; enum { EFFECT_PARABOLA_Y }; };
float Travel(float fYOffset, float amount, float boost, float brake, float wave, float period, float boomerang) {
    if(fYOffset<0) return fYOffset;
    float fAccels[5]={boost,brake,wave,period,boomerang};
    float fEffects[1]={amount};
    float fYAdjust=0, fPeakYOffsetOut=0;
    bool bIsPastPeakOut=false;
  if (fAccels[PlayerOptions::ACCEL_BOOST] != 0) {
    float fEffectHeight = GetNoteFieldHeight();
    float fNewYOffset =
        fYOffset * 1.5f / ((fYOffset + fEffectHeight / 1.2f) / fEffectHeight);
    float fAccelYAdjust =
        fAccels[PlayerOptions::ACCEL_BOOST] * (fNewYOffset - fYOffset);
    // TRICKY: Clamp this value, or else BOOST+BOOMERANG will draw a ton of
    // arrows on the screen.
    rage_clamp(fAccelYAdjust, BOOST_MOD_MIN_CLAMP, BOOST_MOD_MAX_CLAMP);
    fYAdjust += fAccelYAdjust;
  }
  if (fAccels[PlayerOptions::ACCEL_BRAKE] != 0) {
    float fEffectHeight = GetNoteFieldHeight();
    float fScale = SCALE(fYOffset, 0.f, fEffectHeight, 0, 1.f);
    float fNewYOffset = fYOffset * fScale;
    float fBrakeYAdjust =
        fAccels[PlayerOptions::ACCEL_BRAKE] * (fNewYOffset - fYOffset);
    // TRICKY: Clamp this value the same way as BOOST so that in BOOST+BRAKE,
    // BRAKE doesn't overpower BOOST
    rage_clamp(fBrakeYAdjust, BRAKE_MOD_MIN_CLAMP, BRAKE_MOD_MAX_CLAMP);
    fYAdjust += fBrakeYAdjust;
  }
  if (fAccels[PlayerOptions::ACCEL_WAVE] != 0) {
    fYAdjust +=
        fAccels[PlayerOptions::ACCEL_WAVE] * WAVE_MOD_MAGNITUDE *
        std::sin(
            fYOffset /
            ((fAccels[PlayerOptions::ACCEL_WAVE_PERIOD] * WAVE_MOD_HEIGHT) +
             WAVE_MOD_HEIGHT));
  }

  if (fEffects[PlayerOptions::EFFECT_PARABOLA_Y] != 0) {
    fYAdjust += fEffects[PlayerOptions::EFFECT_PARABOLA_Y] *
                (fYOffset / ARROW_SIZE) * (fYOffset / ARROW_SIZE);
  }

  fYOffset += fYAdjust;

  // Factor in boomerang
  if (fAccels[PlayerOptions::ACCEL_BOOMERANG] != 0) {
    float fPeakAtYOffset =
        SCREEN_HEIGHT *
        BOOMERANG_PEAK_PERCENTAGE;  // zero point of boomerang function
    fPeakYOffsetOut = (-1 * fPeakAtYOffset * fPeakAtYOffset / SCREEN_HEIGHT) +
                      1.5f * fPeakAtYOffset;
    bIsPastPeakOut = fYOffset < fPeakAtYOffset;

    fYOffset = (-1 * fYOffset * fYOffset / SCREEN_HEIGHT) + 1.5f * fYOffset;
  }


    return fYOffset;
}
int main() {
    const float cases[][6]={
        {0,0,0,0,0,0}, {0.75f,0,0,0,0,0}, {-0.75f,0,0,0,0,0},
        {0.00000001f,0,0,0,0,0}, {0.75f,0,0,0.25f,0.5f,0},
        {-0.75f,0.5f,0.25f,-0.5f,-0.25f,0}, {0,0.5f,0.25f,0,0,0},
        {0,0.5f,0.25f,-0.5f,0.5f,0}, {0.75f,-0.5f,-0.25f,0,0,0},
        {0.75f,0.5f,0.25f,0.5f,0.25f,1}, {-0.75f,0,0,0,0,-1},
        {0,0.5f,0.25f,0,0,1}, {0,0,0,0.00000001f,0,0}
    };
    for(auto& c:cases) for(float travel:{-128.f,0.f,32.f,128.f,384.f,768.f})
        std::printf("{\"amount\":%.9g,\"boost\":%.9g,\"brake\":%.9g,\"wave\":%.9g,\"period\":%.9g,\"boomerang\":%.9g,\"travel\":%.9g,\"y\":%.9g}\n",c[0],c[1],c[2],c[3],c[4],c[5],travel,Travel(travel,c[0],c[1],c[2],c[3],c[4],c[5]));
}
