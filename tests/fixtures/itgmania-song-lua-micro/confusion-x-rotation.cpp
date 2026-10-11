#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float PI = 3.14159265358979323846f;
struct PlayerOptions {
  enum { EFFECT_CONFUSION_X, EFFECT_CONFUSION_X_OFFSET, EFFECT_ROLL };
  float m_fEffects[3] = {};
  float m_fConfusionX[16] = {};
};
struct PlayerState { struct { float m_fSongBeatVisible=0; } m_Position; };
PlayerOptions options;
PlayerOptions* curr_options = &options;
struct ArrowEffects {
  static float ReceptorGetRotationX(const PlayerState*,int);
  static float GetRotationX(const PlayerState*,float,bool,int);
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
int main() {
  PlayerState state;
  for (float offset : {0.0f, PI/2, -PI, 3.14f}) {
    for (float roll : {0.0f, 2.5f}) {
      options.m_fEffects[PlayerOptions::EFFECT_CONFUSION_X_OFFSET]=offset;
      options.m_fEffects[PlayerOptions::EFFECT_ROLL]=roll;
      std::printf("{\"offset\":%.9g,\"roll\":%.9g,\"travel\":128,\"receptor\":%.9g,\"note\":%.9g,\"hold_cap\":%.9g}\n",
        offset, roll, ArrowEffects::ReceptorGetRotationX(&state,0),
        ArrowEffects::GetRotationX(&state,128,false,0),
        ArrowEffects::GetRotationX(&state,128,true,0));
    }
  }
}
