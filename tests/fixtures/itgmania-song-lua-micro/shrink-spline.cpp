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

constexpr float PI_180R=57.29577951308232f;
enum { NCSM_Disabled,NCSM_Offset,NCSM_Position };
struct RageVector3 { float x=0,y=0,z=0; };
struct ZoomHandler { int m_spline_mode;float point;void EvalForBeat(float,float,RageVector3& out){out={point,point,point};} };
struct Actor {
 RageVector3 zoom;void SetX(float){}void SetY(float){}void SetZ(float){}
 void SetRotationX(float){}void SetRotationY(float){}void SetRotationZ(float){}
 void SetZoomX(float f){zoom.x=f;}void SetZoomY(float f){zoom.y=f;}void SetZoomZ(float f){zoom.z=f;}
};
struct NoteColumnRenderArgs {
 ZoomHandler* zoom_handler;float song_beat=0;
 void spae_zoom_for_beat(const PlayerState*,float,RageVector3&,RageVector3&,int,float)const;
 void SetPRZForActor(Actor*,const RageVector3&,const RageVector3&,const RageVector3&,const RageVector3&,const RageVector3&,const RageVector3&)const;
};
void NoteColumnRenderArgs::spae_zoom_for_beat(
    const PlayerState* state, float beat, RageVector3& sp_zoom,
    RageVector3& ae_zoom, int col_num, float y_offset) const {
  switch (zoom_handler->m_spline_mode) {
    case NCSM_Disabled:
      ae_zoom.x = ae_zoom.y = ae_zoom.z =
          ArrowEffects::GetZoom(state, y_offset, col_num);
      break;
    case NCSM_Offset:
      ae_zoom.x = ae_zoom.y = ae_zoom.z =
          ArrowEffects::GetZoom(state, y_offset, col_num);
      zoom_handler->EvalForBeat(song_beat, beat, sp_zoom);
      break;
    case NCSM_Position:
      zoom_handler->EvalForBeat(song_beat, beat, sp_zoom);
      break;
    default:
      break;
  }
}
void NoteColumnRenderArgs::SetPRZForActor(
    Actor* actor, const RageVector3& sp_pos, const RageVector3& ae_pos,
    const RageVector3& sp_rot, const RageVector3& ae_rot,
    const RageVector3& sp_zoom, const RageVector3& ae_zoom) const {
  actor->SetX(sp_pos.x + ae_pos.x);
  actor->SetY(sp_pos.y + ae_pos.y);
  actor->SetZ(sp_pos.z + ae_pos.z);
  actor->SetRotationX(sp_rot.x * PI_180R + ae_rot.x);
  actor->SetRotationY(sp_rot.y * PI_180R + ae_rot.y);
  actor->SetRotationZ(sp_rot.z * PI_180R + ae_rot.z);
  actor->SetZoomX(sp_zoom.x + ae_zoom.x);
  actor->SetZoomY(sp_zoom.y + ae_zoom.y);
  actor->SetZoomZ(sp_zoom.z + ae_zoom.z);
}
int main() {
 for(int mode:{NCSM_Disabled,NCSM_Offset,NCSM_Position})for(float field:{.5f,1.f,1.5f})
 for(float point:{-.75f,0.f,.6875f,.875f})for(float travel:{0.f,32.f,128.f})for(float linear:{0.f,.75f}) {
  options=PlayerOptions{};options.m_fEffects[PlayerOptions::EFFECT_SHRINK_TO_LINEAR]=linear;
  PlayerState state{field};ZoomHandler handler{mode,point};NoteColumnRenderArgs args{&handler};
  RageVector3 sp,ae,zero;args.spae_zoom_for_beat(&state,0,sp,ae,0,travel);Actor actor;
  args.SetPRZForActor(&actor,zero,zero,zero,zero,sp,ae);
  std::printf("{\"mode\":%d,\"field\":%.9g,\"point\":%.9g,\"travel\":%.9g,\"linear\":%.9g,\"zoom\":%.9g}\n",mode,field,point,travel,linear,actor.zoom.x);
 }
}
