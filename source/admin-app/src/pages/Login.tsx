import { useState } from "react";
import { useNavigate } from "react-router";
import { LoginForm } from "@wardnet/wardnet-web";
import { useBiometric } from "@/hooks/useBiometric";
import { BiometricSetupPrompt } from "@/components/BiometricSetupPrompt";

interface Props {
  onUnlock: () => void;
}

export default function Login({ onUnlock }: Props) {
  const navigate = useNavigate();
  const biometric = useBiometric();
  const [biometricUsername, setBiometricUsername] = useState("");
  const [showBiometricSetup, setShowBiometricSetup] = useState(false);

  function afterLogin() {
    onUnlock();
    navigate("/", { replace: true });
  }

  if (showBiometricSetup) {
    return (
      <BiometricSetupPrompt
        username={biometricUsername}
        onAccept={afterLogin}
        onDecline={afterLogin}
      />
    );
  }

  return (
    <LoginForm
      rememberMe={true}
      onSuccess={(username) => {
        if (biometric.isAvailable() && !biometric.isRegistered()) {
          setBiometricUsername(username);
          setShowBiometricSetup(true);
        } else {
          afterLogin();
        }
      }}
    />
  );
}
