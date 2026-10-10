#include <cmath>
#include <cstdio>
#include <algorithm>
#include <initializer_list>
constexpr float SCREEN_HEIGHT=480, ARROW_SIZE=64;
struct Style { struct ColumnInfo { float fXOffset; }; };
struct PerPlayerData { float m_MinTornado[3][4], m_MaxTornado[3][4]; };
struct PlayerOptions { enum { EFFECT_BOUNCE, EFFECT_BOUNCE_OFFSET, EFFECT_BOUNCE_PERIOD }; bool m_bCosecant=false; };
PlayerOptions options; PlayerOptions* curr_options=&options;
float tornado_position_scale_to_low[3]={-1,-1,-1};
float tornado_position_scale_to_high[3]={1,1,1};
float tornado_offset_frequency[3]={6,6,6};
float tornado_offset_scale_from_low[3]={-1,-1,-1};
float tornado_offset_scale_from_high[3]={1,1,1};
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
float Bounce(float fYOffset,float amount,float offset,float period) {
    float fEffects[3]={amount,offset,period};
    float fPixelOffsetFromCenter=0;
  if (fEffects[PlayerOptions::EFFECT_BOUNCE] != 0) {
    float fBounceAmt = std::abs(
        std::sin(
            ((fYOffset +
              (1.0f * (fEffects[PlayerOptions::EFFECT_BOUNCE_OFFSET]))) /
             (60 + (fEffects[PlayerOptions::EFFECT_BOUNCE_PERIOD] * 60)))));

    fPixelOffsetFromCenter +=
        fEffects[PlayerOptions::EFFECT_BOUNCE] * ARROW_SIZE * 0.5f * fBounceAmt;
  }
    return fPixelOffsetFromCenter;
}
int main() {
  const float cases[][6]={{0.75f,24,0.5f,1,0.25f,0.5f},{-0.5f,-0.4f,0.25f,1,1,0.5f},{-0.25f,-12,-0.5f,-0.75f,-0.5f,-0.5f}};
  for(int columns: {2,4}) {
    Style::ColumnInfo cols[4]; PerPlayerData data{};
    for(int col=0;col<columns;++col) {
      cols[col].fXOffset=(col-(columns-1)*0.5f)*64;
      data.m_MinTornado[0][col]=-(columns-1)*32;
      data.m_MaxTornado[0][col]=(columns-1)*32;
    }
    for(int id=0;id<3;++id) for(int col=0;col<columns;++col) for(float travel: {-128.f,0.f,64.f,128.f,192.f,256.f}) {
      auto &p=cases[id];
      float t=CalculateTornadoOffsetFromMagnitude(0,col,p[3],p[4],p[5],cols,1,data,travel,false);
      float b=Bounce(travel,p[0],p[1],p[2]);
      std::printf("{\"case\":%d,\"columns\":%d,\"column\":%d,\"travel\":%.9g,\"bounce\":%.9g,\"bounce_offset\":%.9g,\"bounce_period\":%.9g,\"tornado\":%.9g,\"tornado_offset\":%.9g,\"tornado_period\":%.9g,\"bounce_extra\":%.9g,\"tornado_extra\":%.9g,\"x\":%.9g}\n",
        id,columns,col,travel,p[0],p[1],p[2],p[3],p[4],p[5],b,t,(t+b)+cols[col].fXOffset);
    }
  }
}
