#include <algorithm>
#include <cfloat>
#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float PI=3.14159265358979323846f, ARROW_SIZE=64, SCREEN_HEIGHT=480;
constexpr float TINY_PERCENT_BASE=.5f,TINY_PERCENT_GATE=1;
constexpr float BEAT_Z_OFFSET_HEIGHT=15,BEAT_Z_PI_HEIGHT=2;
constexpr float DRUNK_Z_COLUMN_FREQUENCY=.2f,DRUNK_Z_OFFSET_FREQUENCY=10,DRUNK_Z_ARROW_MAGNITUDE=.5f;
constexpr int dim_x=0,dim_y=1,dim_z=2;
struct PlayerOptions { enum { EFFECT_ATTENUATE_Z,EFFECT_BEAT_Z,EFFECT_BEAT_Z_PERIOD,EFFECT_BOUNCE_Z,EFFECT_BOUNCE_Z_OFFSET,EFFECT_BOUNCE_Z_PERIOD,EFFECT_BUMPY,EFFECT_BUMPY_OFFSET,EFFECT_BUMPY_PERIOD,EFFECT_DIGITAL_Z,EFFECT_DIGITAL_Z_OFFSET,EFFECT_DIGITAL_Z_PERIOD,EFFECT_DIGITAL_Z_STEPS,EFFECT_DRUNK_Z,EFFECT_DRUNK_Z_OFFSET,EFFECT_DRUNK_Z_PERIOD,EFFECT_DRUNK_Z_SPEED,EFFECT_PARABOLA_Z,EFFECT_SAWTOOTH,EFFECT_SAWTOOTH_PERIOD,EFFECT_SAWTOOTH_Z,EFFECT_SAWTOOTH_Z_PERIOD,EFFECT_SQUARE_Z,EFFECT_SQUARE_Z_OFFSET,EFFECT_SQUARE_Z_PERIOD,EFFECT_TAN_BUMPY,EFFECT_TAN_BUMPY_OFFSET,EFFECT_TAN_BUMPY_PERIOD,EFFECT_TAN_DIGITAL_Z,EFFECT_TAN_DIGITAL_Z_OFFSET,EFFECT_TAN_DIGITAL_Z_PERIOD,EFFECT_TAN_DIGITAL_Z_STEPS,EFFECT_TAN_DRUNK_Z,EFFECT_TAN_DRUNK_Z_OFFSET,EFFECT_TAN_DRUNK_Z_PERIOD,EFFECT_TAN_DRUNK_Z_SPEED,EFFECT_TAN_TORNADO_Z,EFFECT_TAN_TORNADO_Z_OFFSET,EFFECT_TAN_TORNADO_Z_PERIOD,EFFECT_TINY,EFFECT_TORNADO_Z,EFFECT_TORNADO_Z_OFFSET,EFFECT_TORNADO_Z_PERIOD,EFFECT_TWIRL,EFFECT_ZIGZAG_Z,EFFECT_ZIGZAG_Z_OFFSET,EFFECT_ZIGZAG_Z_PERIOD , EFFECT_COUNT };
 float m_fEffects[EFFECT_COUNT]={},m_fBumpy[16]={};bool m_bCosecant=false,m_bZBuffer=false; };
struct PlayerState { int m_PlayerNumber=0;float m_NotefieldZoom=1; };
struct Style { struct ColumnInfo { float fXOffset; };ColumnInfo m_ColumnInfo[2][16];int m_iColsPerPlayer; };
struct PerPlayerData { float m_MinTornado[3][16],m_MaxTornado[3][16],m_fBeatFactor[3]={}; };
PlayerOptions options;PlayerOptions* curr_options=&options;
PerPlayerData g_EffectData[2];
float tornado_position_scale_to_low[3]={-1,-1,-1},tornado_position_scale_to_high[3]={1,1,1};
float tornado_offset_frequency[3]={6,6,6},tornado_offset_scale_from_low[3]={-1,-1,-1},tornado_offset_scale_from_high[3]={1,1,1};
Style style;
struct GameState { const Style* GetCurrentStyle(int){return &style;} } game;
GameState* GAMESTATE=&game;
struct ArrowEffects { static float GetTime(){return 0;}static float GetZPos(const PlayerState*,int,float);static bool NeedZBuffer(); };
// Disabled effects do not execute these functions in this isolated input scope.
float RageTriangle(float){return 0;}float RageSquare(float){return 0;}
template<typename T> void rage_clamp(T& v,T a,T b){v=std::max(a,std::min(v,b));}
#define SCALE(x, l1, h1, l2, h2) \
  (((x) - (l1)) * ((h2) - (l2)) / ((h1) - (l1)) + (l2))
static float SelectTanType(float angle, bool is_cosec) {
  if (is_cosec) {
    return (1 / std::sin(angle));  // cosecant
  } else {
    return std::tan(angle);
  }
}
static float CalculateTornadoOffsetFromMagnitude(
    int dimension, int col_id, float magnitude, float effect_offset,
    float period, const Style::ColumnInfo* pCols, float field_zoom,
    PerPlayerData& data, float y_offset, bool is_tan) {
  const float real_pixel_offset = pCols[col_id].fXOffset * field_zoom;
  const float position_between = SCALE(
      real_pixel_offset, data.m_MinTornado[dimension][col_id] * field_zoom,
      data.m_MaxTornado[dimension][col_id] * field_zoom,
      tornado_position_scale_to_low[dimension],
      tornado_position_scale_to_high[dimension]);
  float rads = std::acos(position_between);
  float frequency = tornado_offset_frequency[dimension];
  rads += (y_offset + effect_offset) * ((period * frequency) + frequency) /
          SCREEN_HEIGHT;
  float processed_rads =
      is_tan ? SelectTanType(rads, curr_options->m_bCosecant) : std::cos(rads);

  const float adjusted_pixel_offset = SCALE(
      processed_rads, tornado_offset_scale_from_low[dimension],
      tornado_offset_scale_from_high[dimension],
      data.m_MinTornado[dimension][col_id] * field_zoom,
      data.m_MaxTornado[dimension][col_id] * field_zoom);
  return (adjusted_pixel_offset - real_pixel_offset) * magnitude;
}
static float CalculateDigitalAngle(float y_offset, float offset, float period) {
  return PI * (y_offset + (1.0f * offset)) /
         (ARROW_SIZE + (period * ARROW_SIZE));
}
static float CalculateBumpyAngle(float y_offset, float offset, float period) {
  return (y_offset + (100.0f * offset)) / ((period * 16.0f) + 16.0f);
}
static float CalculateDrunkAngle(
    float speed, int col, float offset, float col_frequency, float y_offset,
    float period, float offset_frequency) {
  float time = ArrowEffects::GetTime();
  return time * (1 + speed) + col * ((offset * col_frequency) + col_frequency) +
         y_offset * ((period * offset_frequency) + offset_frequency) /
             SCREEN_HEIGHT;
}
float ArrowEffects::GetZPos(
    const PlayerState* pPlayerState, int iCol, float fYOffset) {
  float fZPos = 0;
  const float* fEffects = curr_options->m_fEffects;
  const Style* pStyle =
      GAMESTATE->GetCurrentStyle(pPlayerState->m_PlayerNumber);

  // TODO: Don't index by PlayerNumber.
  const Style::ColumnInfo* pCols =
      pStyle->m_ColumnInfo[pPlayerState->m_PlayerNumber];
  PerPlayerData& data = g_EffectData[pPlayerState->m_PlayerNumber];

  if (fEffects[PlayerOptions::EFFECT_TORNADO_Z] != 0) {
    fZPos += CalculateTornadoOffsetFromMagnitude(
        dim_z, iCol, fEffects[PlayerOptions::EFFECT_TORNADO_Z],
        fEffects[PlayerOptions::EFFECT_TORNADO_Z_OFFSET],
        fEffects[PlayerOptions::EFFECT_TORNADO_Z_PERIOD], pCols,
        pPlayerState->m_NotefieldZoom, data, fYOffset, false);
  }

  if (fEffects[PlayerOptions::EFFECT_TAN_TORNADO_Z] != 0) {
    fZPos += CalculateTornadoOffsetFromMagnitude(
        dim_z, iCol, fEffects[PlayerOptions::EFFECT_TAN_TORNADO_Z],
        fEffects[PlayerOptions::EFFECT_TAN_TORNADO_Z_OFFSET],
        fEffects[PlayerOptions::EFFECT_TAN_TORNADO_Z_PERIOD], pCols,
        pPlayerState->m_NotefieldZoom, data, fYOffset, true);
  }

  if (fEffects[PlayerOptions::EFFECT_BUMPY] != 0) {
    fZPos += fEffects[PlayerOptions::EFFECT_BUMPY] * 40 *
             std::sin(CalculateBumpyAngle(
                 fYOffset, fEffects[PlayerOptions::EFFECT_BUMPY_OFFSET],
                 fEffects[PlayerOptions::EFFECT_BUMPY_PERIOD]));
  }

  if (curr_options->m_fBumpy[iCol] != 0) {
    fZPos += curr_options->m_fBumpy[iCol] * 40 *
             std::sin(CalculateBumpyAngle(
                 fYOffset, fEffects[PlayerOptions::EFFECT_BUMPY_OFFSET],
                 fEffects[PlayerOptions::EFFECT_BUMPY_PERIOD]));
  }

  if (fEffects[PlayerOptions::EFFECT_TAN_BUMPY] != 0) {
    fZPos += fEffects[PlayerOptions::EFFECT_TAN_BUMPY] * 40 *
             SelectTanType(
                 CalculateBumpyAngle(
                     fYOffset, fEffects[PlayerOptions::EFFECT_TAN_BUMPY_OFFSET],
                     fEffects[PlayerOptions::EFFECT_TAN_BUMPY_PERIOD]),
                 curr_options->m_bCosecant);
  }

  if (fEffects[PlayerOptions::EFFECT_ZIGZAG_Z] != 0) {
    float fResult = RageTriangle(
        (PI * (1 / (fEffects[PlayerOptions::EFFECT_ZIGZAG_Z_PERIOD] + 1)) *
         ((fYOffset +
           (100.0f * (fEffects[PlayerOptions::EFFECT_ZIGZAG_Z_OFFSET]))) /
          ARROW_SIZE)));

    fZPos +=
        (fEffects[PlayerOptions::EFFECT_ZIGZAG_Z] * ARROW_SIZE / 2) * fResult;
  }

  if (fEffects[PlayerOptions::EFFECT_SAWTOOTH_Z] != 0) {
    fZPos +=
        (fEffects[PlayerOptions::EFFECT_SAWTOOTH_Z] * ARROW_SIZE) *
        ((0.5f / (fEffects[PlayerOptions::EFFECT_SAWTOOTH_Z_PERIOD] + 1) *
          fYOffset) /
             ARROW_SIZE -
         std::floor(
             (0.5f / (fEffects[PlayerOptions::EFFECT_SAWTOOTH_Z_PERIOD] + 1) *
              fYOffset) /
             ARROW_SIZE));
  }

  if (fEffects[PlayerOptions::EFFECT_PARABOLA_Z] != 0) {
    fZPos += fEffects[PlayerOptions::EFFECT_PARABOLA_Z] *
             (fYOffset / ARROW_SIZE) * (fYOffset / ARROW_SIZE);
  }

  if (fEffects[PlayerOptions::EFFECT_ATTENUATE_Z] != 0) {
    const float fXOffset = pCols[iCol].fXOffset;
    fZPos += fEffects[PlayerOptions::EFFECT_ATTENUATE_Z] *
             (fYOffset / ARROW_SIZE) * (fYOffset / ARROW_SIZE) *
             (fXOffset / ARROW_SIZE);
  }

  if (fEffects[PlayerOptions::EFFECT_DRUNK_Z] != 0) {
    fZPos += fEffects[PlayerOptions::EFFECT_DRUNK_Z] *
             (std::cos(CalculateDrunkAngle(
                  fEffects[PlayerOptions::EFFECT_DRUNK_Z_SPEED], iCol,
                  fEffects[PlayerOptions::EFFECT_DRUNK_Z_OFFSET],
                  DRUNK_Z_COLUMN_FREQUENCY, fYOffset,
                  fEffects[PlayerOptions::EFFECT_DRUNK_Z_PERIOD],
                  DRUNK_Z_OFFSET_FREQUENCY)) *
              ARROW_SIZE * DRUNK_Z_ARROW_MAGNITUDE);
  }

  if (fEffects[PlayerOptions::EFFECT_TAN_DRUNK_Z] != 0) {
    fZPos += fEffects[PlayerOptions::EFFECT_TAN_DRUNK_Z] *
             (SelectTanType(
                  CalculateDrunkAngle(
                      fEffects[PlayerOptions::EFFECT_TAN_DRUNK_Z_SPEED], iCol,
                      fEffects[PlayerOptions::EFFECT_TAN_DRUNK_Z_OFFSET],
                      DRUNK_Z_COLUMN_FREQUENCY, fYOffset,
                      fEffects[PlayerOptions::EFFECT_TAN_DRUNK_Z_PERIOD],
                      DRUNK_Z_OFFSET_FREQUENCY),
                  curr_options->m_bCosecant) *
              ARROW_SIZE * DRUNK_Z_ARROW_MAGNITUDE);
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

  if (fEffects[PlayerOptions::EFFECT_DIGITAL_Z] != 0) {
    fZPos += (fEffects[PlayerOptions::EFFECT_DIGITAL_Z] * ARROW_SIZE * 0.5f) *
             std::round(
                 (fEffects[PlayerOptions::EFFECT_DIGITAL_Z_STEPS] + 1) *
                 std::sin(CalculateDigitalAngle(
                     fYOffset, fEffects[PlayerOptions::EFFECT_DIGITAL_Z_OFFSET],
                     fEffects[PlayerOptions::EFFECT_DIGITAL_Z_PERIOD]))) /
             (fEffects[PlayerOptions::EFFECT_DIGITAL_Z_STEPS] + 1);
  }

  if (fEffects[PlayerOptions::EFFECT_TAN_DIGITAL_Z] != 0) {
    fZPos +=
        (fEffects[PlayerOptions::EFFECT_TAN_DIGITAL_Z] * ARROW_SIZE * 0.5f) *
        std::round(
            (fEffects[PlayerOptions::EFFECT_TAN_DIGITAL_Z_STEPS] + 1) *
            SelectTanType(
                CalculateDigitalAngle(
                    fYOffset,
                    fEffects[PlayerOptions::EFFECT_TAN_DIGITAL_Z_OFFSET],
                    fEffects[PlayerOptions::EFFECT_TAN_DIGITAL_Z_PERIOD]),
                curr_options->m_bCosecant)) /
        (fEffects[PlayerOptions::EFFECT_TAN_DIGITAL_Z_STEPS] + 1);
  }

  if (fEffects[PlayerOptions::EFFECT_SQUARE_Z] != 0) {
    float fResult = RageSquare(
        (PI *
         (fYOffset +
          (1.0f * (fEffects[PlayerOptions::EFFECT_SQUARE_Z_OFFSET]))) /
         (ARROW_SIZE +
          (fEffects[PlayerOptions::EFFECT_SQUARE_Z_PERIOD] * ARROW_SIZE))));
    fZPos += (fEffects[PlayerOptions::EFFECT_SQUARE_Z] * ARROW_SIZE * 0.5f) *
             fResult;
  }

  if (fEffects[PlayerOptions::EFFECT_BOUNCE_Z] != 0) {
    float fBounceAmt = std::abs(
        std::sin(
            ((fYOffset +
              (1.0f * (fEffects[PlayerOptions::EFFECT_BOUNCE_Z_OFFSET]))) /
             (60 + (fEffects[PlayerOptions::EFFECT_BOUNCE_Z_PERIOD] * 60)))));

    fZPos += fEffects[PlayerOptions::EFFECT_BOUNCE_Z] * ARROW_SIZE * 0.5f *
             fBounceAmt;
  }

  return fZPos;
}
bool ArrowEffects::NeedZBuffer() {
  const float* fEffects = curr_options->m_fEffects;
  // We also need to use the Z buffer if twirl is in play, because of
  // hold modulation. -vyhd (OpenITG r623)
  if (fEffects[PlayerOptions::EFFECT_BUMPY] != 0 ||
      fEffects[PlayerOptions::EFFECT_TWIRL] != 0) {
    return true;
  }
  if (fEffects[PlayerOptions::EFFECT_BEAT_Z] != 0 ||
      fEffects[PlayerOptions::EFFECT_DIGITAL_Z] != 0) {
    return true;
  }
  if (fEffects[PlayerOptions::EFFECT_ZIGZAG_Z] != 0 ||
      fEffects[PlayerOptions::EFFECT_SAWTOOTH_Z] != 0) {
    return true;
  }
  if (fEffects[PlayerOptions::EFFECT_PARABOLA_Z] != 0 ||
      fEffects[PlayerOptions::EFFECT_SQUARE_Z] != 0) {
    return true;
  }
  if (curr_options->m_bZBuffer ||
      fEffects[PlayerOptions::EFFECT_ATTENUATE_Z] != 0) {
    return true;
  }
  if (fEffects[PlayerOptions::EFFECT_BOUNCE_Z] != 0) {
    return true;
  }
  return false;
}
void Init() {
 const Style* pStyle=&style;const Style::ColumnInfo* pCols=style.m_ColumnInfo[0];
 PerPlayerData& data=g_EffectData[0];
  bool wide_field = pStyle->m_iColsPerPlayer > 4;
  int max_player_col = pStyle->m_iColsPerPlayer - 1;
  for (int dimension = 0; dimension < 3; ++dimension) {
    int width = 3;
    // wide_field only matters for x, which is dimension 0. -Kyz
    if (dimension == 0 && wide_field) {
      width = 2;
    }
    for (int col_id = 0; col_id <= max_player_col; ++col_id) {
      int start_col = col_id - width;
      int end_col = col_id + width;
      rage_clamp(start_col, 0, max_player_col);
      rage_clamp(end_col, 0, max_player_col);
      data.m_MinTornado[dimension][col_id] = FLT_MAX;
      data.m_MaxTornado[dimension][col_id] = FLT_MIN;
      for (int i = start_col; i <= end_col; ++i) {
        // Using the x offset when the dimension might be y or z feels so
        // wrong, but it provides min and max values when otherwise the
        // limits would just be zero, which would make it do nothing. -Kyz
        data.m_MinTornado[dimension][col_id] =
            std::min(pCols[i].fXOffset, data.m_MinTornado[dimension][col_id]);
        data.m_MaxTornado[dimension][col_id] =
            std::max(pCols[i].fXOffset, data.m_MaxTornado[dimension][col_id]);
      }
    }

  }
}
float SawX(const PlayerState* pPlayerState,int iColNum,float fYOffset) {
 const float* fEffects=curr_options->m_fEffects;const auto* pCols=style.m_ColumnInfo[0];
 float fPixelOffsetFromCenter=0;
  if (fEffects[PlayerOptions::EFFECT_SAWTOOTH] != 0) {
    fPixelOffsetFromCenter +=
        (fEffects[PlayerOptions::EFFECT_SAWTOOTH] * ARROW_SIZE) *
        ((0.5f / (fEffects[PlayerOptions::EFFECT_SAWTOOTH_PERIOD] + 1) *
          fYOffset) /
             ARROW_SIZE -
         std::floor(
             (0.5f / (fEffects[PlayerOptions::EFFECT_SAWTOOTH_PERIOD] + 1) *
              fYOffset) /
             ARROW_SIZE));
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

 return fPixelOffsetFromCenter;
}
int main() {
 // One effect at a time, mixed effects, negative strengths and sub-epsilon gates.
 const float configs[][14]={
   {0,0,0,0,0,0,0,0,0,0,0,0,0,0},
   {.75f,-16,.5f,0,0,0,0,0,0,0,0,0,0,0},
   {0,0,0,-.5f,8,-.25f,2,0,0,0,0,0,0,0},
   {0,0,0,0,0,0,0,1.25f,-32,.5f,0,0,0,0},
   {0,0,0,0,0,0,0,0,0,0,-.75f,1.25f,.5f,-.5f},
   {.75f,-16,.5f,-.5f,8,-.25f,2,1.25f,-32,.5f,-.75f,1.25f,.5f,-.5f},
   {-.25f,16,-.25f,.75f,-8,.5f,-.5f,-.5f,32,-.25f,.5f,-.25f,-.75f,.25f},
   {1e-8f,0,0,-1e-8f,0,0,0,1e-8f,0,0,-1e-8f,0,1e-8f,0}
 };
 const int keys[]={PlayerOptions::EFFECT_BOUNCE_Z,PlayerOptions::EFFECT_BOUNCE_Z_OFFSET,PlayerOptions::EFFECT_BOUNCE_Z_PERIOD,
   PlayerOptions::EFFECT_DIGITAL_Z,PlayerOptions::EFFECT_DIGITAL_Z_OFFSET,PlayerOptions::EFFECT_DIGITAL_Z_PERIOD,PlayerOptions::EFFECT_DIGITAL_Z_STEPS,
   PlayerOptions::EFFECT_TORNADO_Z,PlayerOptions::EFFECT_TORNADO_Z_OFFSET,PlayerOptions::EFFECT_TORNADO_Z_PERIOD,
   PlayerOptions::EFFECT_SAWTOOTH_Z,PlayerOptions::EFFECT_SAWTOOTH_Z_PERIOD,PlayerOptions::EFFECT_SAWTOOTH,PlayerOptions::EFFECT_SAWTOOTH_PERIOD};
 const char* names[]={"bounce_z","bounce_z_offset","bounce_z_period","digital_z","digital_z_offset","digital_z_period","digital_z_steps",
   "tornado_z","tornado_z_offset","tornado_z_period","sawtooth_z","sawtooth_z_period","sawtooth","sawtooth_period"};
 for(int count:{1,2,4,8}) for(float zoom:{.75f,1.f,1.5f}) {
   style.m_iColsPerPlayer=count;
   for(int col=0;col<count;++col)style.m_ColumnInfo[0][col].fXOffset=(col-(count-1)*.5f)*64;
   Init();PlayerState state;state.m_NotefieldZoom=zoom;
   for(const auto& c:configs) for(float tiny:{-.5f,0.f,.5f}) for(float travel:{-256.f,-128.f,-.25f,0.f,.25f,32.f,64.f,128.f,256.f})
   for(int col=0;col<count;++col) {
     options=PlayerOptions{};for(int key=0;key<14;++key)options.m_fEffects[keys[key]]=c[key];
     options.m_fEffects[PlayerOptions::EFFECT_TINY]=tiny;
     std::printf("{\"columns\":%d,\"col\":%d,\"col_x\":%.9g,\"zoom\":%.9g,\"tiny\":%.9g,\"travel\":%.9g,",count,col,style.m_ColumnInfo[0][col].fXOffset,zoom,tiny,travel);
     for(int key=0;key<14;++key)std::printf("\"%s\":%.9g,",names[key],c[key]);
     std::printf("\"x\":%.9g,\"z\":%.9g,\"receptor_x\":%.9g,\"receptor_z\":%.9g,\"depth\":%s}\n",
       SawX(&state,col,travel),ArrowEffects::GetZPos(&state,col,travel),SawX(&state,col,0),ArrowEffects::GetZPos(&state,col,0),ArrowEffects::NeedZBuffer()?"true":"false");
   }
 }
}
