import { useSharedBackendState } from "./hooks/useSharedBackendState";
import { MainApp } from "./layout/MainApp";

export default function App() {
  useSharedBackendState();
  return <MainApp />;
}
