#include <cmath>
#include <cstdio>
#include <cfloat>
#include <algorithm>
#include <initializer_list>
constexpr float PI=3.14159265358979323846f,ARROW_SPACING=64,ARROW_SIZE=64,SCREEN_HEIGHT=480;
constexpr float BOOST_MOD_MIN_CLAMP=-400,BOOST_MOD_MAX_CLAMP=400,BRAKE_MOD_MIN_CLAMP=-400,BRAKE_MOD_MAX_CLAMP=400;
constexpr float WAVE_MOD_MAGNITUDE=20,WAVE_MOD_HEIGHT=38,BOOMERANG_PEAK_PERCENTAGE=.75f;
constexpr float EXPAND_MULTIPLIER_FREQUENCY=3,EXPAND_MULTIPLIER_SCALE_FROM_LOW=-1,EXPAND_MULTIPLIER_SCALE_FROM_HIGH=1;
constexpr float EXPAND_MULTIPLIER_SCALE_TO_LOW=.75f,EXPAND_MULTIPLIER_SCALE_TO_HIGH=1.75f;
constexpr float EXPAND_SPEED_SCALE_FROM_LOW=0,EXPAND_SPEED_SCALE_FROM_HIGH=1,EXPAND_SPEED_SCALE_TO_LOW=1;
#define SCALE(x, l1, h1, l2, h2) \
  (((x) - (l1)) * ((h2) - (l2)) / ((h1) - (l1)) + (l2))
template<class T>void rage_clamp(T& x,T l,T h){x=std::clamp(x,l,h);}
struct SongPosition {float m_fSongBeatVisible=0,m_fMusicSecondsVisible=0;bool m_bFreeze=false,m_bDelay=false;};
struct PlayerOptions {enum {ACCEL_BOOST,ACCEL_BRAKE,ACCEL_WAVE,ACCEL_WAVE_PERIOD,ACCEL_EXPAND,ACCEL_EXPAND_PERIOD,ACCEL_TAN_EXPAND,ACCEL_TAN_EXPAND_PERIOD,ACCEL_BOOMERANG};enum{EFFECT_PARABOLA_Y};
 float m_fAccels[9]={},m_fEffects[1]={};float m_fTimeSpacing=0,m_fScrollSpeed=1,m_fMaxScrollBPM=0,m_fScrollBPM=60,m_fRandomSpeed=0;bool m_bCosecant=false;
};
struct PlayerState {SongPosition m_Position;int m_PlayerNumber=0;float m_fReadBPM=60;const SongPosition& GetDisplayedPosition()const{return m_Position;}};
struct Timing {float GetElapsedTimeFromBeat(float beat){return beat;}float GetDisplayedSpeedPercent(float,float){return 1;}};
struct Steps {Timing timing;Timing* GetTimingData(){return &timing;}};
struct SongOptions {float m_fMusicRate=1;};struct SongOptionState {SongOptions current;SongOptions& GetCurrent(){return current;}};
struct GameState {Steps* m_pCurSteps[1];bool m_bInStepEditor=false;SongOptionState m_SongOptions;unsigned m_iStageSeed=0;};
Steps steps;GameState game{{&steps}};GameState* GAMESTATE=&game;
PlayerOptions options;PlayerOptions* curr_options=&options;
struct PerPlayerData {float m_fExpandSeconds=0,m_fTanExpandSeconds=0;};PerPlayerData g_EffectData[1];
float GetNoteFieldHeight(){return 480;}
float SelectTanType(float f,bool c){return c?1/std::sin(f):std::tan(f);}
int BeatToNoteRow(float beat){return (int)std::round(beat*48);}
struct ArrowEffects {static float GetYOffset(const PlayerState*,int,float,float&,bool&,bool);static float GetDisplayedBeat(const PlayerState*,float);};
float ArrowEffects::GetDisplayedBeat(const PlayerState*,float beat){return beat;}
float ArrowEffects::GetYOffset(
    const PlayerState* pPlayerState, int iCol, float fNoteBeat,
    float& fPeakYOffsetOut, bool& bIsPastPeakOut, bool bAbsolute) {
  // Default values that are returned if boomerang is off.
  fPeakYOffsetOut = FLT_MAX;
  bIsPastPeakOut = true;

  float fYOffset = 0;
  const SongPosition& position = pPlayerState->GetDisplayedPosition();

  float fSongBeat = position.m_fSongBeatVisible;

  Steps* pCurSteps = GAMESTATE->m_pCurSteps[pPlayerState->m_PlayerNumber];

  /* Usually, fTimeSpacing is 0 or 1, in which case we use entirely beat spacing
   * or entirely time spacing (respectively). Occasionally, we tween between
   * them. */
  if (curr_options->m_fTimeSpacing != 1.0f) {
    if (GAMESTATE->m_bInStepEditor) {
      // Use constant spacing in step editor
      fYOffset = fNoteBeat - fSongBeat;
    } else {
      fYOffset = GetDisplayedBeat(pPlayerState, fNoteBeat) -
                 GetDisplayedBeat(pPlayerState, fSongBeat);
      fYOffset *= pCurSteps->GetTimingData()->GetDisplayedSpeedPercent(
          position.m_fSongBeatVisible, position.m_fMusicSecondsVisible);
    }
    fYOffset *= 1 - curr_options->m_fTimeSpacing;
  }

  if (curr_options->m_fTimeSpacing != 0.0f) {
    float fSongSeconds = pPlayerState->m_Position.m_fMusicSecondsVisible;
    float fNoteSeconds =
        pCurSteps->GetTimingData()->GetElapsedTimeFromBeat(fNoteBeat);
    float fSecondsUntilStep = fNoteSeconds - fSongSeconds;
    float fBPM = curr_options->m_fScrollBPM;
    float fBPS =
        fBPM / 60.f / GAMESTATE->m_SongOptions.GetCurrent().m_fMusicRate;
    float fYOffsetTimeSpacing = fSecondsUntilStep * fBPS;
    fYOffset += fYOffsetTimeSpacing * curr_options->m_fTimeSpacing;
  }

  // TODO: If we allow noteskins to have metricable row spacing
  // (per issue 24), edit this to reflect that. -aj
  fYOffset *= ARROW_SPACING;

  // Factor in scroll speed
  float fScrollSpeed = curr_options->m_fScrollSpeed;
  if (curr_options->m_fMaxScrollBPM != 0) {
    fScrollSpeed = curr_options->m_fMaxScrollBPM /
                   (pPlayerState->m_fReadBPM *
                    GAMESTATE->m_SongOptions.GetCurrent().m_fMusicRate);
  }

  // don't mess with the arrows after they've crossed 0
  if (fYOffset < 0) {
    return fYOffset * fScrollSpeed;
  }

  const float* fAccels = curr_options->m_fAccels;
  const float* fEffects = curr_options->m_fEffects;

  // TODO: Don't index by PlayerNumber.
  PerPlayerData& data = g_EffectData[pPlayerState->m_PlayerNumber];

  float fYAdjust = 0;  // fill this in depending on PlayerOptions

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

  if (curr_options->m_fRandomSpeed > 0 && !bAbsolute) {
    // Generate a deterministically "random" speed for each arrow.
    unsigned seed = GAMESTATE->m_iStageSeed + (BeatToNoteRow(fNoteBeat) << 8) +
                    (iCol * 100);

    for (int i = 0; i < 3; ++i) {
      seed = ((seed * 1664525u) + 1013904223u) & 0xFFFFFFFF;
    }
    float fRandom = seed / 4294967296.0f;

    /* Random speed always increases speed: a random speed of 10 indicates
     * [1,11]. This keeps it consistent with other mods: 0 means no effect. */
    fScrollSpeed *=
        SCALE(fRandom, 0.0f, 1.0f, 1.0f, curr_options->m_fRandomSpeed + 1.0f);
  }

  if (fAccels[PlayerOptions::ACCEL_EXPAND] != 0) {
    float fExpandMultiplier = SCALE(
        std::cos(
            data.m_fExpandSeconds * EXPAND_MULTIPLIER_FREQUENCY *
            (fAccels[PlayerOptions::ACCEL_EXPAND_PERIOD] + 1)),
        EXPAND_MULTIPLIER_SCALE_FROM_LOW, EXPAND_MULTIPLIER_SCALE_FROM_HIGH,
        EXPAND_MULTIPLIER_SCALE_TO_LOW, EXPAND_MULTIPLIER_SCALE_TO_HIGH);
    fScrollSpeed *= SCALE(
        fAccels[PlayerOptions::ACCEL_EXPAND], EXPAND_SPEED_SCALE_FROM_LOW,
        EXPAND_SPEED_SCALE_FROM_HIGH, EXPAND_SPEED_SCALE_TO_LOW,
        fExpandMultiplier);
  }

  if (fAccels[PlayerOptions::ACCEL_TAN_EXPAND] != 0) {
    float fTanExpandMultiplier = SCALE(
        SelectTanType(
            data.m_fTanExpandSeconds * EXPAND_MULTIPLIER_FREQUENCY *
                (fAccels[PlayerOptions::ACCEL_TAN_EXPAND_PERIOD] + 1),
            curr_options->m_bCosecant),
        EXPAND_MULTIPLIER_SCALE_FROM_LOW, EXPAND_MULTIPLIER_SCALE_FROM_HIGH,
        EXPAND_MULTIPLIER_SCALE_TO_LOW, EXPAND_MULTIPLIER_SCALE_TO_HIGH);
    fScrollSpeed *= SCALE(
        fAccels[PlayerOptions::ACCEL_TAN_EXPAND], EXPAND_SPEED_SCALE_FROM_LOW,
        EXPAND_SPEED_SCALE_FROM_HIGH, EXPAND_SPEED_SCALE_TO_LOW,
        fTanExpandMultiplier);
  }

  fYOffset *= fScrollSpeed;
  fPeakYOffsetOut *= fScrollSpeed;

  return fYOffset;
}
float StepPhase(float start,float period,float delta,bool freeze,bool delay) {
  PerPlayerData data;data.m_fExpandSeconds=start;SongPosition position;position.m_bFreeze=freeze;position.m_bDelay=delay;
  float values[9]={};values[PlayerOptions::ACCEL_EXPAND_PERIOD]=period;const float* accels=values;
  double fTime=delta,fLastTime=0;
    if (!position.m_bFreeze || !position.m_bDelay) {
      data.m_fExpandSeconds += static_cast<float>(fTime - fLastTime);
      data.m_fExpandSeconds = std::fmod(
          data.m_fExpandSeconds,
          (PI * 2) / (accels[PlayerOptions::ACCEL_EXPAND_PERIOD] + 1));
      data.m_fTanExpandSeconds += static_cast<float>(fTime - fLastTime);
      data.m_fTanExpandSeconds = std::fmod(
          data.m_fTanExpandSeconds,
          (PI * 2) / (accels[PlayerOptions::ACCEL_TAN_EXPAND_PERIOD] + 1));
    }

  return data.m_fExpandSeconds;
}
void Print(float f){if(std::isnan(f))std::printf("\"nan\"");else if(std::isinf(f))std::printf(std::signbit(f)?"\"-inf\"":"\"inf\"");else std::printf("%.9g",f);}
int main(){
 for(int seq:{0,1,2})for(int flags:{0,1,2,3}){
  float seconds=0;
  for(int frame=0;frame<12;++frame){
   const float periods[]={0,.5f,-.5f,1.75f,-1.f,-1.5f,0,1e-8f,-.75f,2.f,-.125f,0};
   const float deltas[]={0,.016666668f,.25f,1.f,7.f,2.f,.05f,.016666668f,.125f,8.f,.25f,0};
   float period=seq==0?0:seq==1?.5f:periods[frame];float start=seconds,delta=deltas[frame];bool freeze=flags&1,delay=flags&2;
   seconds=StepPhase(seconds,period,delta,freeze,delay);
   for(float strength:{-.5f,.75f,1.5f})for(float raw:{-128.f,0.f,.25f,64.f,256.f})for(float speed:{.5f,1.f,2.f}){
    options=PlayerOptions{};options.m_fScrollSpeed=speed;options.m_fAccels[PlayerOptions::ACCEL_EXPAND]=strength;
    options.m_fAccels[PlayerOptions::ACCEL_EXPAND_PERIOD]=period;g_EffectData[0].m_fExpandSeconds=seconds;
    PlayerState state;float peak;bool past;float y=ArrowEffects::GetYOffset(&state,0,raw/64,peak,past,false);float tail=ArrowEffects::GetYOffset(&state,0,(raw+128)/64,peak,past,false);
    std::printf("{\"sequence\":%d,\"flags\":%d,\"frame\":%d,\"start\":",seq,flags,frame);Print(start);
    std::printf(",\"delta\":%.9g,\"period\":%.9g,\"seconds\":",delta,period);Print(seconds);
    std::printf(",\"strength\":%.9g,\"raw\":%.9g,\"speed\":%.9g,\"y\":",strength,raw,speed);Print(y);std::printf(",\"tail_y\":");Print(tail);std::printf("}\n");
   }
  }
 }
}
