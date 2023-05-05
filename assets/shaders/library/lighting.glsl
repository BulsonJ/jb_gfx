struct Light{
    vec4 position;
    vec3 colour;
    float intensity;
};

layout(std140,set = 1, binding = 1) uniform LightBuffer{
    Light lights[4];
} lightData;

vec3 CalculateDirectionalLight(vec3 normal, vec3 worldPos, vec3 cameraPos, vec3 lightDir, vec3 lightColour, float lightStrength) {
    float diff = max(dot(normal, lightDir), 0.0);
    vec3 diffuse = diff * (lightColour * lightStrength);

    // Specular
    float shininess = 32.0;
    float specularStrength = 0.2;
    vec3 viewDir = normalize(cameraPos - worldPos);
    vec3 halfwayDir = normalize(lightDir + viewDir);
    float spec = pow(max(dot(normal, halfwayDir), 0.0), shininess);
    vec3 specular = specularStrength * spec * (lightColour * lightStrength);

    return diffuse + specular;
}

vec3 CalculatePointLight(vec3 normal, vec3 worldPos, vec3 cameraPos, Light light) {
    vec3 lightDir = normalize(light.position.xyz - worldPos);
    float diff = max(dot(normal, lightDir), 0.0);
    vec3 diffuse = diff * (light.colour * light.intensity);

    // Specular
    float shininess = 32.0;
    float specularStrength = 0.2;
    vec3 viewDir = normalize(cameraPos - worldPos);
    vec3 halfwayDir = normalize(lightDir + viewDir);
    float spec = pow(max(dot(normal, halfwayDir), 0.0), shininess);
    vec3 specular = specularStrength * spec * (light.colour * light.intensity);

    // attenuation
    float distance    = length(light.position.xyz - worldPos);
    float lightConstant = 1.0;
    float lightLinear = 0.09;
    float lightQuadratic = 0.032;
    float attenuation = 1.0 / (lightConstant + lightLinear * distance + lightQuadratic * (distance * distance));

    diffuse *= attenuation;
    specular *= attenuation;

    return diffuse + specular;
}