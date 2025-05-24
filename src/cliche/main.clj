(ns cliche.main (:require [babashka.cli :as cli]))

(def cli-options {:port {:default 80 :coerce :long} :help {:coerce :boolean}})

(defn -main [& _args]
  (prn (cli/parse-opts *command-line-args* {:spec cli-options})))

(comment (cli/parse-opts [:port 700] {:spec cli-options}))
