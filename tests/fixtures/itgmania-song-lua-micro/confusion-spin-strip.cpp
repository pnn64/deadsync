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
struct RageVector3 { float x=0,y=0,z=0; RageVector3()=default; RageVector3(float a,float b,float c):x(a),y(b),z(c) {} };
struct RageVector4 { float x=0,y=0,z=0,w=0; RageVector4()=default; RageVector4(float a,float b,float c,float d):x(a),y(b),z(c),w(d) {} };
void RageQuatMultiply(RageVector4*,const RageVector4&,const RageVector4&);
void RageVec3Cross(
    RageVector3* ret, const RageVector3* a, const RageVector3* b) {
  ret->x = (a->y * b->z) - (a->z * b->y);
  ret->y = ((a->x * b->z) - (a->z * b->x));
  ret->z = (a->x * b->y) - (a->y * b->x);
}
void RageAARotate(RageVector3* inret, const RageVector3* axis, float angle) {
  float ha = angle / 2.0f;
  float ca2 = std::cos(ha);
  float sa2 = std::sin(ha);
  RageVector4 quat(axis->x * sa2, axis->y * sa2, axis->z * sa2, ca2);
  RageVector4 quatc(-quat.x, -quat.y, -quat.z, ca2);
  RageVector4 point(inret->x, inret->y, inret->z, 0.0f);
  RageQuatMultiply(&point, quat, point);
  RageQuatMultiply(&point, point, quatc);
  inret->x = point.x;
  inret->y = point.y;
  inret->z = point.z;
}
void RageQuatMultiply(
    RageVector4* pOut, const RageVector4& pA, const RageVector4& pB) {
  RageVector4 out;
  out.x = pA.w * pB.x + pA.x * pB.w + pA.y * pB.z - pA.z * pB.y;
  out.y = pA.w * pB.y + pA.y * pB.w + pA.z * pB.x - pA.x * pB.z;
  out.z = pA.w * pB.z + pA.z * pB.w + pA.x * pB.y - pA.y * pB.x;
  out.w = pA.w * pB.w - pA.x * pB.x - pA.y * pB.y - pA.z * pB.z;

  float dist, square;

  square = out.x * out.x + out.y * out.y + out.z * out.z + out.w * out.w;

  if (square > 0.0) {
    dist = 1.0f / std::sqrt(square);
  } else {
    dist = 1;
  }

  out.x *= dist;
  out.y *= dist;
  out.z *= dist;
  out.w *= dist;

  *pOut = out;
}
RageVector3 StripOffset(const PlayerState* state,float travel) {
    const RageVector3 sp_rot(0,0,0), ae_rot(0,ArrowEffects::GetRotationY(state,travel,0)*-(PI/180),0);
    const RageVector3 render_forward(0,1,0),pos_y_vec(0,1,0),pos_z_vec(0,0,1);
    const float fScaledFrameWidth=64;
    const float render_roty = (sp_rot.y + ae_rot.y);

    // (step 2 of vector handling)
    RageVector3 render_left;
    if (std::abs(render_forward.z) > 0.9f)  // 0.9 arbitrariliy picked.
    {
      RageVec3Cross(&render_left, &pos_y_vec, &render_forward);
    } else {
      RageVec3Cross(&render_left, &pos_z_vec, &render_forward);
    }
    RageAARotate(&render_left, &render_forward, render_roty);
    const float half_width = fScaledFrameWidth * .5f;
    render_left.x *= half_width;
    render_left.y *= half_width;
    render_left.z *= half_width;
    return render_left;
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
    const auto offset=StripOffset(&state,travel);
    std::printf("{\"strength_x\":%.9g,\"strength_y\":%.9g,\"offset_x\":%.9g,\"offset_y\":%.9g,"
      "\"beat\":%.9g,\"travel\":%.9g,\"hold_cap\":%s,\"roll\":%.9g,\"twirl\":%.9g,"
      "\"dx\":%.9g,\"dy\":%.9g,\"dz\":%.9g}\n",
      a[0],a[1],a[2],a[3],beat,travel,cap?"true":"false",mix*1.25f,mix*-.75f,offset.x,offset.y,offset.z);

  }
}
