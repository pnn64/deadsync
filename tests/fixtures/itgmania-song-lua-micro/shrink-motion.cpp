#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float ARROW_SIZE=64;
struct PlayerOptions { enum { EFFECT_PULSE_INNER,EFFECT_PULSE_OUTER,EFFECT_PULSE_OFFSET,EFFECT_PULSE_PERIOD,EFFECT_SHRINK_TO_MULT,EFFECT_SHRINK_TO_LINEAR,EFFECT_TINY };
 float m_fEffects[7]={},m_fTiny[16]={}; };
struct PlayerState { float m_NotefieldZoom; };
PlayerOptions options;PlayerOptions* curr_options=&options;
struct ArrowEffects { static float GetZoom(const PlayerState*,float,int);static float GetZoomVariable(float,int,float);static float GetPulseInner(); };
float ArrowEffects::GetZoom(
    const PlayerState* pPlayerState, float fYOffset, int iCol) {
  float fZoom = 1.0f;
  // Design change:  Instead of having a flag in the style that toggles a
  // fixed zoom (0.6) that is only applied to the columns, ScreenGameplay now
  // calculates a zoom factor to apply to the notefield and puts it in the
  // PlayerState. -Kyz
  fZoom *= pPlayerState->m_NotefieldZoom;

  fZoom = GetZoomVariable(fYOffset, iCol, fZoom);

  float fTinyPercent = curr_options->m_fEffects[PlayerOptions::EFFECT_TINY];
  if (fTinyPercent != 0) {
    fTinyPercent = std::pow(0.5f, fTinyPercent);
    fZoom *= fTinyPercent;
  }
  if (curr_options->m_fTiny[iCol] != 0) {
    fTinyPercent = std::pow(0.5f, curr_options->m_fTiny[iCol]);
    fZoom *= fTinyPercent;
  }
  return fZoom;
}
float ArrowEffects::GetZoomVariable(float fYOffset, int iCol, float fCurZoom) {
  float fZoom = fCurZoom;
  if (curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_INNER] != 0 ||
      curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_OUTER] != 0) {
    float sine = std::sin((
        (fYOffset +
         (100.0f *
          (curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_OFFSET]))) /
        (0.4f * (ARROW_SIZE +
                 (curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_PERIOD] *
                  ARROW_SIZE)))));

    fZoom *=
        (sine *
         (curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_OUTER] * 0.5f)) +
        GetPulseInner();
  }
  if (curr_options->m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_MULT] != 0 &&
      fYOffset >= 0) {
    fZoom *=
        1 /
        (1 + (fYOffset *
              (curr_options->m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_MULT] /
               100.0f)));
  }

  if (curr_options->m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_LINEAR] != 0 &&
      fYOffset >= 0) {
    fZoom += fYOffset *
             (0.5f *
              curr_options->m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_LINEAR] /
              ARROW_SIZE);
  }
  return fZoom;
}
float ArrowEffects::GetPulseInner() {
  float fPulseInner = 1.0f;
  if (curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_INNER] != 0 ||
      curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_OUTER] != 0) {
    fPulseInner =
        ((curr_options->m_fEffects[PlayerOptions::EFFECT_PULSE_INNER] * 0.5f) +
         1);
    if (fPulseInner == 0) {
      fPulseInner = 0.01f;
    }
  }
  return fPulseInner;
}
void PrintZoom(float value) {
 if(std::isnan(value)) std::printf("\"nan\"");
 else if(std::isinf(value))std::printf(std::signbit(value)?"\"-inf\"":"\"inf\"");
 else std::printf("%.9g",value);
}
int main() {
 const float configs[][6]={
 {0,0,0,0,0,0},{.75f,-.125f,0,0,0,0},{-.75f,.25f,0,0,0,0},
 {.5f,.5f,.5f,-.75f,.1f,-.25f},{-1.5f,-.125f,-2.f,.75f,-.1f,.5f},
 {1e-8f,-1e-8f,0,0,0,0},{0,-.125f,0,0,0,0}};
 for(const auto& c:configs)for(float travel:{-128.f,-.25f,0.f,.25f,32.f,64.f,128.f,256.f,800.f})
 for(float field:{-.5f,0.f,.5f,1.f,1.5f})for(float tiny:{-.5f,0.f,.5f})for(float lane_tiny:{-.5f,0.f,.75f}) {
   options=PlayerOptions{};options.m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_LINEAR]=c[0];options.m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_MULT]=c[1];
   options.m_fEffects[PlayerOptions::EFFECT_PULSE_INNER]=c[2];options.m_fEffects[PlayerOptions::EFFECT_PULSE_OUTER]=c[3];
   options.m_fEffects[PlayerOptions::EFFECT_PULSE_OFFSET]=c[4];options.m_fEffects[PlayerOptions::EFFECT_PULSE_PERIOD]=c[5];
   options.m_fEffects[PlayerOptions::EFFECT_TINY]=tiny;options.m_fTiny[0]=lane_tiny;PlayerState state{field};
   std::printf("{\"linear\":%.9g,\"mult\":%.9g,\"inner\":%.9g,\"outer\":%.9g,\"offset\":%.9g,\"period\":%.9g,\"travel\":%.9g,\"field\":%.9g,\"tiny\":%.9g,\"lane_tiny\":%.9g,\"zoom\":",
     c[0],c[1],c[2],c[3],c[4],c[5],travel,field,tiny,lane_tiny);
   PrintZoom(ArrowEffects::GetZoom(&state,travel,0));std::printf(",\"receptor_zoom\":");PrintZoom(ArrowEffects::GetZoom(&state,0,0));std::printf("}\n");
 }
}
