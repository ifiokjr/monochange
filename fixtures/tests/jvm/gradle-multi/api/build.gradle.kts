plugins {
    id("java-library")
}

group = "com.example"
version = "1.0.0"

dependencies {
    api("com.example:core:1.0.0")
    implementation("com.google.guava:guava:33.0.0-jre")
    compileOnly("org.projectlombok:lombok:1.18.30")
    testImplementation("junit:junit:4.13.2")
}
