layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
    mat4 proj;
    mat4 view;
    mat4 invProjView;
    vec4 cameraPos;
    vec4 ambientLight;
    vec3 directionalLightColour;
    float directionalLightStrength;
    vec4 directionalLightDirection;
    mat4 sunProj;
    mat4 sunView;
    int pointLightCount;
    int padding[3];
} cameraData;
