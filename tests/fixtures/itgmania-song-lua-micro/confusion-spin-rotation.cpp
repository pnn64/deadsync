#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float PI=3.14159265358979323846f;
struct PlayerOptions {
  enum { EFFECT_CONFUSION_X, EFFECT_CONFUSION_X_OFFSET, EFFECT_CONFUSION_Y, EFFECT_CONFUSION_Y_OFFSET, EFFECT_ROLL, EFFECT_TWIRL };
  float m_fEffects[6]={};
  float m_fConfusionX[16]={},m_fConfusionY[16]={};
};
struct PlayerState { struct { float m_fSongBeatVisible; } m_Position; };
PlayerOptions options;
PlayerOptions* curr_options=&options;
struct ArrowEffects {
  static float ReceptorGetRotationX(const PlayerState*,int);
  static float ReceptorGetRotationY(const PlayerState*,int);
  static float GetRotationX(const PlayerState*,float,bool,int);
  static float GetRotationY(const PlayerState*,float,int);
};
float ArrowEffects::ReceptorGetRotationX(
    const PlayerState* pPlayerState, int iCol) {
  const float* fEffects = curr_options->m_fEffects;
  float fRotation = 0;

  if (curr_options->m_fConfusionX[iCol] != 0) {
    fRotation += curr_options->m_fConfusionX[iCol] * 180.0f / PI;
  }

  if (fEffects[PlayerOptions::EFFECT_CONFUSION_X_OFFSET] != 0) {
    fRotation +=
        fEffects[PlayerOptions::EFFECT_CONFUSION_X_OFFSET] * 180.0f / PI;
  }

  if (fEffects[PlayerOptions::EFFECT_CONFUSION_X] != 0) {
    float fConfRotation = pPlayerState->m_Position.m_fSongBeatVisible;
    fConfRotation *= fEffects[PlayerOptions::EFFECT_CONFUSION_X];
    fConfRotation = std::fmod(fConfRotation, 2 * PI);
    fConfRotation *= -180 / PI;
    fRotation += fConfRotation;
  }

  return fRotation;
}

float ArrowEffects::ReceptorGetRotationY(
    const PlayerState* pPlayerState, int iCol) {
  const float* fEffects = curr_options->m_fEffects;
  float fRotation = 0;

  if (curr_options->m_fConfusionY[iCol] != 0) {
    fRotation += curr_options->m_fConfusionY[iCol] * 180.0f / PI;
  }

  if (fEffects[PlayerOptions::EFFECT_CONFUSION_Y_OFFSET] != 0) {
    fRotation +=
        fEffects[PlayerOptions::EFFECT_CONFUSION_Y_OFFSET] * 180.0f / PI;
  }

  if (fEffects[PlayerOptions::EFFECT_CONFUSION_Y] != 0) {
    float fConfRotation = pPlayerState->m_Position.m_fSongBeatVisible;
    fConfRotation *= fEffects[PlayerOptions::EFFECT_CONFUSION_Y];
    fConfRotation = std::fmod(fConfRotation, 2 * PI);
    fConfRotation *= -180 / PI;
    fRotation += fConfRotation;
  }

  return fRotation;
}

float ArrowEffects::GetRotationX(
    const PlayerState* pPlayerState, float fYOffset, bool bIsHoldCap,
    int iCol) {
  const float* fEffects = curr_options->m_fEffects;
  float fRotation = 0;
  if (fEffects[PlayerOptions::EFFECT_CONFUSION_X] != 0 ||
      fEffects[PlayerOptions::EFFECT_CONFUSION_X_OFFSET] != 0 ||
      curr_options->m_fConfusionX[iCol] != 0) {
    fRotation += ReceptorGetRotationX(pPlayerState, iCol);
  }
  if (fEffects[PlayerOptions::EFFECT_ROLL] != 0 && !bIsHoldCap) {
    fRotation += fEffects[PlayerOptions::EFFECT_ROLL] * fYOffset / 2;
  }
  return fRotation;
}

float ArrowEffects::GetRotationY(
    const PlayerState* pPlayerState, float fYOffset, int iCol) {
  const float* fEffects = curr_options->m_fEffects;
  float fRotation = 0;
  if (fEffects[PlayerOptions::EFFECT_CONFUSION_Y] != 0 ||
      fEffects[PlayerOptions::EFFECT_CONFUSION_Y_OFFSET] != 0 ||
      curr_options->m_fConfusionY[iCol] != 0) {
    fRotation += ReceptorGetRotationY(pPlayerState, iCol);
  }
  if (fEffects[PlayerOptions::EFFECT_TWIRL] != 0) {
    fRotation += fEffects[PlayerOptions::EFFECT_TWIRL] * fYOffset / 2;
  }
  return fRotation;
}
int main() {
  const float configs[][4]={{0,0,0,0},{.75f,-.5f,-.2f,.4f},{-.25f,.75f,.2f,-.4f},
    {2.5f,-2.5f,0,0},{1e-8f,-1e-8f,0,0},{0,0,.4f,-.7f}};
  for(const auto& a:configs) for(float beat:{-12.f,-.25f,0.f,.3f,1.f,6.5f,14.f})
  for(float travel:{-128.f,0.f,64.f,128.f}) for(bool cap:{false,true}) for(float mix:{0.f,1.f}) {
    PlayerState state{{beat}};
    options.m_fEffects[PlayerOptions::EFFECT_CONFUSION_X]=a[0];
    options.m_fEffects[PlayerOptions::EFFECT_CONFUSION_Y]=a[1];
    options.m_fEffects[PlayerOptions::EFFECT_CONFUSION_X_OFFSET]=a[2];
    options.m_fEffects[PlayerOptions::EFFECT_CONFUSION_Y_OFFSET]=a[3];
    options.m_fEffects[PlayerOptions::EFFECT_ROLL]=mix*1.25f;
    options.m_fEffects[PlayerOptions::EFFECT_TWIRL]=mix*-.75f;
    std::printf("{\"strength_x\":%.9g,\"strength_y\":%.9g,\"offset_x\":%.9g,\"offset_y\":%.9g,"
      "\"beat\":%.9g,\"travel\":%.9g,\"hold_cap\":%s,\"roll\":%.9g,\"twirl\":%.9g,"
      "\"receptor_x\":%.9g,\"receptor_y\":%.9g,\"note_x\":%.9g,\"note_y\":%.9g}\n",
      a[0],a[1],a[2],a[3],beat,travel,cap?"true":"false",mix*1.25f,mix*-.75f,
      ArrowEffects::ReceptorGetRotationX(&state,0),ArrowEffects::ReceptorGetRotationY(&state,0),
      ArrowEffects::GetRotationX(&state,travel,cap,0),ArrowEffects::GetRotationY(&state,travel,0));
  }
}
